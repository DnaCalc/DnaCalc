//! dc-rif (D4): DNA Calc's workbook and controller over calc-host, through the route the skins run:
//! a new workbook and an Excel-saved one are opened, edited by cell entries, recalculated,
//! subscribed to, undone and redone, counted by provenance and saved.

// Case names are fixed identities of the form `<bead>__<case>`.
#![allow(non_snake_case)]

#[path = "../../../crates/calc-engine/tests/fixtures/seven_cell.rs"]
mod seven_cell;

#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn contract_seven_cell_scope_yields_and_delivers_receipts_through_the_wire() {
    use dna_calc_worker::{
        server::{Server, answer_text},
        wire::{
            self, Area, CalculationEvent as C, CalculationOutcome, Event, FromServer, Intent, Key,
            Reply, Request, ToServer,
        },
    };
    let mut server = Server::new();
    server.set_calculation_budget(calc_host::WorkBudget::evaluations(1));
    let sheet = server.book().sheets()[0].id;
    let cell = |row, col| CellKey::new(sheet, row, col).unwrap();
    let send = |server: &mut Server, request: Request| {
        let request = if request.is_command() {
            server.envelope(request)
        } else {
            request
        };
        let (text, _) = answer_text(server, &wire::encode(&ToServer { id: 1, request }), None);
        wire::decode::<FromServer>(&text).unwrap()
    };
    assert!(matches!(
        send(
            &mut server,
            Request::Intent(Intent::SetCalcMode { manual: true })
        ),
        FromServer::Reply { .. }
    ));
    for &(row, col, entry, _) in seven_cell::CELLS {
        assert!(
            matches!(send(&mut server,Request::Intent(Intent::EnterCell{at:Key::of(cell(row,col)),text:entry.into()})),
            FromServer::Reply{reply:Reply::Done{outcome,..},..} if outcome.accepted)
        );
    }
    server.take_events();
    let areas: Vec<_> = seven_cell::PURE_SCOPE
        .iter()
        .map(|&(row, col)| Area::of(GridArea::cell(cell(row, col))))
        .collect();
    assert!(
        matches!(send(&mut server,Request::Intent(Intent::CalculateRange{areas:areas.clone()})),
        FromServer::Reply{reply:Reply::Done{outcome,..},..} if outcome.accepted)
    );
    assert!(server.has_pending_calculation());
    let mut events = server.take_events();
    assert!(events.iter().any(|event|matches!(event,Event::Calculation{receipt:C::Admitted{scope:wire::CalculationScope::Range(scope),selection:wire::CalculationSelection::ForceAll,..},..} if *scope==areas)));
    assert!(events.iter().any(|event| matches!(
        event,
        Event::Calculation {
            receipt: C::Yielded { .. },
            ..
        }
    )));
    let revision = server.book().host().revision();
    let history = server.book().session().history_depth();
    assert!(matches!(
        send(&mut server, Request::Ping),
        FromServer::Reply {
            reply: Reply::Pong { .. },
            ..
        }
    ));
    assert!(
        server.has_pending_calculation(),
        "Ping does not drive or replace the active calculation"
    );
    for _ in 0..64 {
        if !server.has_pending_calculation() {
            break;
        }
        server.advance_calculation();
        events.extend(server.take_events());
    }
    assert!(!server.has_pending_calculation());
    assert_eq!(server.book().host().revision(), revision);
    assert_eq!(server.book().session().history_depth(), history);
    for (row, col, value) in [(1, 2, "2"), (1, 3, "4"), (1, 5, "1"), (10, 7, "1")] {
        assert_eq!(shown(server.book(), cell(row, col)).text, value);
    }
    assert!(
        !matches!(
            shown(server.book(), cell(1, 4)).provenance,
            Provenance::Calculated
        ),
        "D1's P01/P03 callback is not part of the pure scope"
    );
    let admitted = events
        .iter()
        .position(|event| {
            matches!(
                event,
                Event::Calculation {
                    receipt: C::Admitted { .. },
                    ..
                }
            )
        })
        .unwrap();
    let published = events
        .iter()
        .position(|event| {
            matches!(
                event,
                Event::Calculation {
                    receipt: C::Published(_),
                    ..
                }
            )
        })
        .unwrap();
    let completed = events
        .iter()
        .position(|event| {
            matches!(
                event,
                Event::Calculation {
                    receipt: C::Completed {
                        outcome: CalculationOutcome::Complete,
                        calculated: 4,
                        refused: 0,
                        ..
                    },
                    ..
                }
            )
        })
        .unwrap();
    assert!(admitted < published && published < completed);
    let first_command = match &events[admitted] {
        Event::Calculation {
            receipt: C::Admitted { command, .. },
            ..
        } => *command,
        _ => unreachable!(),
    };
    // A C1-only replay needs the P02 out-of-scope B1 cache. This check explicitly requests
    // the same pure scope, so equal values need no unimplemented boundary-cache policy.
    let replay_scope = areas.clone();
    assert!(matches!(
        send(&mut server, Request::Intent(Intent::CalculateRange { areas: replay_scope.clone() })),
        FromServer::Reply { reply: Reply::Done { outcome, .. }, .. } if outcome.accepted
    ));
    for _ in 0..64 {
        if !server.has_pending_calculation() {
            break;
        }
        server.advance_calculation();
    }
    assert!(!server.has_pending_calculation());
    let replay = server.take_events();
    let replay_command = replay
        .iter()
        .find_map(|event| match event {
            Event::Calculation {
                receipt:
                    C::Admitted {
                        command,
                        scope: wire::CalculationScope::Range(scope),
                        ..
                    },
                ..
            } if *scope == replay_scope => Some(*command),
            _ => None,
        })
        .unwrap();
    assert_ne!(first_command, replay_command);
    assert_eq!(shown(server.book(), cell(1, 3)).text, "4");
    let replay_publication = replay.iter().position(|event| matches!(event,
        Event::Calculation { receipt: C::Published(publication), .. } if publication.command == replay_command
    )).unwrap();
    let replay_complete = replay.iter().position(|event| matches!(event,
        Event::Calculation { receipt: C::Completed { command, calculated: 4, outcome: CalculationOutcome::Complete, .. }, .. }
            if *command == replay_command
    )).unwrap();
    assert!(
        replay_publication < replay_complete,
        "equal values still publish calculation status"
    );
    // SliceV1 salvages only dirty successors. Request that selection explicitly; the Manual
    // CalculateRange command above and below continues to normalize to ForceAll.
    for (col, text) in [(1, "10"), (6, "2")] {
        assert!(
            matches!(send(&mut server,Request::Intent(Intent::EnterCell{at:Key::of(cell(1,col)),text:text.into()})),
            FromServer::Reply{reply:Reply::Done{outcome,..},..} if outcome.accepted)
        );
    }
    send(
        &mut server,
        Request::Intent(Intent::Calculate {
            scope: wire::CalculationScope::Range(areas.clone()),
            selection: wire::CalculationSelection::DirtyAndVolatile,
        }),
    );
    let mut successor_events = server.take_events();
    let predecessor = successor_events
        .iter()
        .find_map(|event| match event {
            Event::Calculation {
                receipt:
                    C::Admitted {
                        command,
                        selection: wire::CalculationSelection::DirtyAndVolatile,
                        ..
                    },
                ..
            } => Some(*command),
            _ => None,
        })
        .unwrap();
    for _ in 0..2 {
        server.advance_calculation();
        successor_events.extend(server.take_events());
    }
    assert!(server.has_pending_calculation());
    assert!(
        matches!(send(&mut server,Request::Intent(Intent::EnterCell{at:Key::of(cell(1,1)),text:"20".into()})),
        FromServer::Reply{reply:Reply::Done{outcome,..},..} if outcome.accepted)
    );
    successor_events.extend(server.take_events());
    for _ in 0..64 {
        if !server.has_pending_calculation() {
            break;
        }
        server.advance_calculation();
        successor_events.extend(server.take_events());
    }
    assert!(!server.has_pending_calculation());
    for (row, col, text) in [
        (1, 2, "21"),
        (1, 3, "42"),
        (1, 5, "1"),
        (2, 5, "2"),
        (10, 7, "1"),
    ] {
        assert_eq!(shown(server.book(), cell(row, col)).text, text);
    }
    assert!(successor_events.iter().any(|event|matches!(event,Event::Calculation{
        receipt:C::Reused{at,computed_by,certified_for},..} if *at==Key::of(cell(1,5)) && *computed_by==predecessor && *certified_for!=predecessor)));
    assert!(
        successor_events
            .iter()
            .any(|event| matches!(event,Event::Calculation{
        receipt:C::Rejected{at},..} if *at==Key::of(cell(1,2))))
    );
    assert!(!successor_events.iter().any(|event|matches!(event,Event::Calculation{
        receipt:C::Completed{command,outcome:CalculationOutcome::Complete,..},..} if *command==predecessor)));
    // A client may replace the book before draining its completed control messages. They
    // carry the old workbook identity and must remain deliverable exactly once.
    let again = send(
        &mut server,
        Request::Intent(Intent::CalculateRange { areas }),
    );
    assert!(
        matches!(again,FromServer::Reply{reply:Reply::Done{outcome,..},..} if outcome.accepted)
    );
    for _ in 0..64 {
        if !server.has_pending_calculation() {
            break;
        }
        server.advance_calculation();
    }
    let old_workbook = server.book().host().revision();
    assert!(matches!(
        send(&mut server, Request::New),
        FromServer::Reply { .. }
    ));
    let retained = server.take_events();
    assert!(retained.iter().any(
        |event| matches!(event,Event::Calculation{receipt:C::Completed{revision,..},..}
        if *revision==old_workbook.get())
    ));
    assert!(
        !retained
            .iter()
            .any(|event| matches!(event, Event::Tiles(_)))
    );
    assert!(server.take_events().is_empty());
}

use calc_host::value::{CellKey, GridArea, SheetId};
use calc_host::{DeltaMirror, Interest, SkinIntent};
use dna_calc::controller::{Controller, EditOrigin};
use dna_calc_worker::cells::{self, Provenance};
use dna_calc_worker::workbook::{Workbook, host_options};
use leptos::prelude::*;

#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn dc_971p_4__navigation_range_entry_and_refusal_use_the_worker_message_route() {
    use dna_calc_worker::{
        server::{Server, answer_text},
        wire::{self, Area, FromServer, Intent, Key, Reply, Request, ToServer},
    };
    let mut server = Server::new();
    let sheet = server.book().sheets()[0].id;
    let cell = |row, col| CellKey::new(sheet, row, col).unwrap();
    let area = GridArea::new(cell(1, 2), cell(3, 2)).unwrap();
    let send = |server: &mut Server, intent| {
        let text = wire::encode(&ToServer {
            id: 1,
            request: server.envelope(Request::Intent(intent)),
        });
        let (answer, _) = answer_text(server, &text, None);
        let FromServer::Reply {
            reply: Reply::Done { outcome, .. },
            ..
        } = wire::decode(&answer).unwrap()
        else {
            panic!("unexpected reply {answer}")
        };
        outcome
    };
    let refused = send(
        &mut server,
        Intent::EnterSelection {
            text: "9".to_owned(),
        },
    );
    assert!(!refused.accepted && !refused.committed);
    assert_eq!(shown(server.book(), cell(1, 1)).text, "");
    for (row, text) in [(1, "2"), (2, "3"), (3, "4")] {
        assert!(
            send(
                &mut server,
                Intent::EnterCell {
                    at: Key::of(cell(row, 1)),
                    text: text.to_owned()
                }
            )
            .accepted
        );
    }
    assert!(
        send(
            &mut server,
            Intent::Select {
                active: Key::of(cell(1, 2)),
                areas: vec![Area::of(area)]
            }
        )
        .accepted
    );
    let filled = send(
        &mut server,
        Intent::EnterSelection {
            text: "=A1*10+$A$1".to_owned(),
        },
    );
    assert!(filled.accepted && filled.committed);
    for (row, value) in [(1, "22"), (2, "32"), (3, "42")] {
        let cell = shown(server.book(), cell(row, 2));
        assert_eq!(cell.text, value);
        assert_eq!(cell.provenance, Provenance::Calculated);
    }
    let moved = send(
        &mut server,
        Intent::Navigate {
            nav: dna_calc::grid::selection::Navigation::Walk {
                horizontal: false,
                reverse: false,
            },
            extend: false,
            page_height: 400,
        },
    );
    assert_eq!(
        moved.selection,
        Some((Key::of(cell(2, 2)), vec![Area::of(area)]))
    );
    assert!(send(&mut server, Intent::Undo).accepted);
    for row in 1..=3 {
        assert_eq!(shown(server.book(), cell(row, 2)).text, "");
    }
    assert!(send(&mut server, Intent::Redo).accepted);
    assert_eq!(shown(server.book(), cell(3, 2)).text, "42");
    let whole = GridArea::new(cell(1, 2), cell(calc_host::value::MAX_ROWS, 2)).unwrap();
    assert!(
        send(
            &mut server,
            Intent::Select {
                active: Key::of(cell(1, 2)),
                areas: vec![Area::of(whole)]
            }
        )
        .accepted
    );
    let refused = send(
        &mut server,
        Intent::EnterSelection {
            text: "9".to_owned(),
        },
    );
    assert!(!refused.accepted && !refused.committed);
    assert_eq!(shown(server.book(), cell(3, 2)).text, "42");

    let opened = server.execute_current(
        Request::Open {
            name: "data-validation.xlsx".to_owned(),
        },
        Some(
            include_bytes!("../../../crates/calc-host/tests/fixtures/dc_6yw/data-validation.xlsx")
                .to_vec(),
        ),
    );
    assert!(opened.reply.is_ok());
    let sheet = server.book().sheets()[0].id;
    let first = CellKey::new(sheet, 1, 1).unwrap();
    let second = CellKey::new(sheet, 1, 2).unwrap();
    let before = [
        shown(server.book(), first).text,
        shown(server.book(), second).text,
    ];
    let range = GridArea::new(first, second).unwrap();
    assert!(
        send(
            &mut server,
            Intent::Select {
                active: Key::of(first),
                areas: vec![Area::of(range)]
            }
        )
        .accepted
    );
    let refused = send(
        &mut server,
        Intent::EnterSelection {
            text: "not-in-list".to_owned(),
        },
    );
    assert!(!refused.accepted && !refused.committed);
    assert_eq!(
        [
            shown(server.book(), first).text,
            shown(server.book(), second).text
        ],
        before
    );
}

/// calc-doc's D1 fixture, which Excel saved: dynamic arrays, legacy arrays, names and a table.
fn fixture() -> Vec<u8> {
    include_bytes!("../../../crates/calc-doc/tests/fixtures/cdoc_m1_2/d1.saved.xlsx").to_vec()
}

fn at(sheet: SheetId, address: &str) -> CellKey {
    let (row, column) = cells::parse_address(address).expect("an address");
    CellKey::new(sheet, row, column).unwrap()
}

fn sheet(book: &Workbook, name: &str) -> SheetId {
    book.sheets()
        .into_iter()
        .find(|tab| tab.name == name)
        .expect("the sheet")
        .id
}

fn shown(book: &Workbook, at: CellKey) -> cells::CellShown {
    book.shown(&book.cell(at).expect("projected"))
}

fn apply(book: &mut Workbook, mirror: &mut DeltaMirror) {
    for delta in book.take_deltas() {
        mirror.apply(&delta).expect("the deltas follow one another");
    }
}

#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn dc_rif__a_new_workbook_is_edited_through_the_session_and_its_subscription() {
    assert!(host_options().locale.is_some(), "the en-US profile");
    let mut book = Workbook::new_blank();
    assert_eq!(book.name(), "Book1.xlsx");
    let sheet1 = sheet(&book, "Sheet1");
    let area = GridArea::new(at(sheet1, "A1"), at(sheet1, "H20")).unwrap();
    book.subscribe(Interest::Area(area)).expect("subscribed");
    let mut mirror = DeltaMirror::default();
    apply(&mut book, &mut mirror);
    assert_eq!(mirror.cells().count(), 0);

    for (address, text) in [
        ("A1", "2"),
        ("A2", "3"),
        ("A3", "=A1*A2+1"),
        ("B1", "=WEBSERVICE(\"http://example.com\")"),
        ("C1", "={1;2;3}"),
    ] {
        let receipt = book.enter(at(sheet1, address), text);
        assert!(receipt.accepted(), "{address}: {:?}", receipt.error);
    }
    assert!(book.changed());
    apply(&mut book, &mut mirror);
    let profile = book.profile().cloned();
    let seen = |mirror: &DeltaMirror, address: &str| {
        cells::shown(
            mirror.cell(at(sheet1, address)).expect("in the mirror"),
            profile.as_ref(),
            None,
        )
    };
    let a3 = seen(&mirror, "A3");
    assert_eq!(
        (a3.text.as_str(), a3.provenance, a3.formula),
        ("7", Provenance::Calculated, true)
    );
    // A web function is refused (no web access is modelled), and shows as refused.
    let b1 = seen(&mirror, "B1");
    assert_eq!(
        (b1.text.as_str(), b1.provenance),
        ("refused", Provenance::Refused)
    );
    // The spill reaches the mirror.
    assert_eq!(seen(&mirror, "C3").text, "3");
    assert_eq!(seen(&mirror, "A1").provenance, Provenance::Literal);

    // A refused entry changes nothing.
    let receipt = book.enter(at(sheet1, "D1"), "=");
    assert!(!receipt.accepted());

    // Undo and redo.
    book.enter(at(sheet1, "A1"), "5");
    apply(&mut book, &mut mirror);
    assert_eq!(seen(&mirror, "A3").text, "16");
    book.dispatch(SkinIntent::Undo);
    apply(&mut book, &mut mirror);
    assert_eq!(seen(&mirror, "A3").text, "7");
    book.dispatch(SkinIntent::Redo);
    apply(&mut book, &mut mirror);
    assert_eq!(seen(&mirror, "A3").text, "16");

    // Manual calculation leaves a changed formula stale until it is asked for.
    book.dispatch(SkinIntent::SetCalcMode(calc_host::CalcMode::Manual));
    book.enter(at(sheet1, "A2"), "10");
    apply(&mut book, &mut mirror);
    assert_eq!(seen(&mirror, "A3").provenance, Provenance::Stale);
    book.dispatch(SkinIntent::Recalculate);
    apply(&mut book, &mut mirror);
    assert_eq!(seen(&mirror, "A3").text, "51");

    // The census counts every formula, the refused one as refused.
    let census = book.census();
    assert_eq!(census.formulas(), 3);
    assert_eq!(
        (census.calculated(), census.refused(), census.stale()),
        (2, 1, 0)
    );
    assert_eq!(census.sheets[0].used.as_deref(), Some("Sheet1!A1:C3"));
    assert_eq!(census.refusals.values().sum::<usize>(), 1);

    // Saved and reopened, the formulas calculate again.
    let (bytes, _) = book.save().expect("saved");
    assert!(!book.changed());
    let reopened = Workbook::open("Book1.xlsx", bytes).expect("reopens");
    let sheet1 = sheet(&reopened, "Sheet1");
    let a3 = shown(&reopened, at(sheet1, "A3"));
    assert_eq!(
        (a3.text.as_str(), a3.provenance),
        ("51", Provenance::Calculated)
    );
    assert_eq!(
        cells::input_text(reopened.cell(at(sheet1, "A3")).unwrap().input()),
        "=A1*A2+1"
    );
}

#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn dc_rif__an_excel_workbook_opens_calculated_and_counts_its_formulas() {
    let mut book = Workbook::open("d1.saved.xlsx", fixture()).expect("it opens");
    let calc = sheet(&book, "Calc");
    // Opened in automatic mode, the workbook is recalculated: values are the engine's.
    let u3 = shown(&book, at(calc, "U3"));
    assert_eq!(
        (u3.text.as_str(), u3.provenance),
        ("21", Provenance::Calculated)
    );
    assert_eq!(shown(&book, at(calc, "B2")).text, "4");
    let census = book.census();
    // The package's 17 formula elements (16 on Calc, 1 on Data), and the two members of the legacy
    // array L1:L3 beside its anchor, which Excel counts as formula cells; the cells the dynamic
    // arrays spill into are not formula cells.
    assert_eq!(census.formulas(), 19);
    // Every formula cell is calculated or refused; nothing is left as the file stored it.
    assert_eq!(census.file_cached(), 0);
    assert_eq!(census.stale(), 0);
    assert_eq!(census.calculated() + census.refused(), census.formulas());
    assert!(book.used_area(calc).is_some());
}

#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn dc_rif__the_controller_edits_the_active_cell_and_follows_the_selection() {
    let owner = Owner::new();
    owner.with(|| {
        let controller = Controller::with_transport(std::rc::Rc::new(
            dna_calc_worker::transport::InProcessTransport::default(),
        ));
        // The controller's answers come through callbacks (dc-p96); in process they come at once.
        let commit = |controller: &Controller| {
            let accepted = std::rc::Rc::new(std::cell::Cell::new(None));
            let seen = accepted.clone();
            controller.commit_edit(move |ok| seen.set(Some(ok)));
            accepted.get().expect("the server answered")
        };
        let sheet1 = controller.sheet.get_untracked().expect("a sheet");
        assert!(!controller.has_dirty_work());
        assert!(!controller.history_available(false));
        controller.select(2, 1);
        controller.begin_edit(EditOrigin::Cell, Some("4".to_owned()));
        assert!(commit(&controller));
        assert_eq!(controller.editing.get_untracked(), None);
        controller.select(3, 1);
        controller.begin_edit(EditOrigin::Bar, Some("=A2*2".to_owned()));
        assert!(commit(&controller));
        // The application's selection subscription holds the active cell, as the page shows it.
        let a3 = controller
            .selection
            .with_untracked(|mirror| mirror.cell(at(sheet1, "A3")).cloned())
            .expect("the selection holds A3");
        assert_eq!(a3.provenance, Provenance::Calculated);
        assert_eq!(a3.text, "8");
        assert_eq!(a3.input, "=A2*2");
        // A refused entry keeps the edit open and says why.
        controller.begin_edit(EditOrigin::Cell, Some("=".to_owned()));
        assert!(!commit(&controller));
        assert_eq!(controller.editing.get_untracked(), Some(EditOrigin::Cell));
        assert!(controller.status.get_untracked().refused);
        controller.cancel_edit();
        // Undo restores the selection of the edit it takes back.
        controller.select(1, 5);
        controller.undo();
        assert_eq!(controller.active.get_untracked(), (3, 1));
        // Saving names the workbook and its size.
        let saved = std::rc::Rc::new(std::cell::RefCell::new(None));
        let into = saved.clone();
        controller.save(move |name, bytes| *into.borrow_mut() = Some((name, bytes)));
        let (name, bytes) = saved.borrow_mut().take().expect("saved");
        assert!(
            controller.has_dirty_work(),
            "a browser download cannot establish durable storage"
        );
        assert!(controller.download_unconfirmed.get_untracked());
        assert_eq!(name, "Book1.xlsx");
        assert!(
            controller
                .status
                .get_untracked()
                .text
                .contains(&format!("{} bytes", bytes.len()))
        );
        // Opening replaces the workbook: a new generation at the first sheet's A1.
        let generation = controller.generation.get_untracked();
        let opened = std::rc::Rc::new(std::cell::Cell::new(None));
        let seen = opened.clone();
        controller.open("d1.saved.xlsx", fixture(), move |ok| seen.set(Some(ok)));
        assert_eq!(opened.get(), Some(true));
        assert!(!controller.has_dirty_work());
        dna_calc::notebook::ViewState::new(controller)
            .insert_text
            .set("not entered".to_owned());
        assert!(controller.has_dirty_work());
        let downloaded = std::rc::Rc::new(std::cell::Cell::new(false));
        let seen_download = downloaded.clone();
        controller.save(move |_, _| seen_download.set(true));
        assert!(
            !downloaded.get(),
            "a Notebook draft cannot be silently omitted from a download"
        );
        dna_calc::notebook::ViewState::new(controller)
            .insert_text
            .set(String::new());
        assert!(!controller.has_dirty_work());
        assert_eq!(controller.generation.get_untracked(), generation + 1);
        assert_eq!(controller.active.get_untracked(), (1, 1));
        assert!(controller.sheets().iter().any(|tab| tab.name == "Calc"));
        // An undo of an edit on another sheet shows that sheet at the restored cell, whose input
        // the selection holds, so the formula bar shows it.
        let sheets = controller.sheets();
        let data = sheets.iter().find(|tab| tab.name == "Data").unwrap().id;
        let calc = sheets.iter().find(|tab| tab.name == "Calc").unwrap().id;
        controller.show_sheet(data);
        let before = controller.active_cell().map(|cell| cell.input);
        assert!(before.is_some(), "Data!A1 holds an input");
        controller.begin_edit(EditOrigin::Cell, Some("5".to_owned()));
        assert!(commit(&controller));
        controller.show_sheet(calc);
        controller.undo();
        assert_eq!(controller.sheet.get_untracked(), Some(data));
        assert_eq!(controller.active.get_untracked(), (1, 1));
        assert_eq!(controller.active_cell().map(|cell| cell.input), before);
        // A package that is not one is refused, and the workbook stays.
        let opened = std::rc::Rc::new(std::cell::Cell::new(None));
        let seen = opened.clone();
        controller.open("junk.xlsx", b"not a package".to_vec(), move |ok| {
            seen.set(Some(ok))
        });
        assert_eq!(opened.get(), Some(false));
        assert!(controller.status.get_untracked().refused);
        assert!(controller.sheets().iter().any(|tab| tab.name == "Calc"));
        // A reference picked on another sheet never retargets the authored entry.
        controller.select(5, 6);
        controller.begin_edit(EditOrigin::Cell, Some("=".to_owned()));
        controller.show_sheet(calc);
        assert_eq!(controller.editing.get_untracked(), Some(EditOrigin::Bar));
        assert!(controller.point_at(3, 21, false));
        assert_eq!(controller.edit_text.get_untracked(), "=Calc!U3");
        assert!(commit(&controller));
        assert_eq!(controller.sheet.get_untracked(), Some(data));
        assert_eq!(controller.active.get_untracked(), (5, 6));
        assert_eq!(controller.active_cell().unwrap().input, "=Calc!U3");
        assert_eq!(controller.active_cell().unwrap().text, "21");
        let revision = controller.revision.get_untracked();
        controller.begin_edit(EditOrigin::Bar, None);
        assert!(commit(&controller));
        assert_eq!(
            controller.revision.get_untracked(),
            revision,
            "an unchanged edit makes no workbook transaction"
        );
        controller.undo();
        assert_eq!(controller.active_cell().unwrap().input, "4.5");
    });
}

/// Native-only: it writes the saved workbook to the folder `DNACALC_RIF_SAVE_DIR` names, for Excel
/// to open (calc-host's `tests/fixtures/d2/measure-saved-open.ps1`); without the variable it only
/// saves.
#[test]
fn dc_rif__a_new_workbook_saves_for_excel() {
    let mut book = Workbook::new_blank();
    let sheet1 = sheet(&book, "Sheet1");
    for (address, text) in [
        ("A1", "2"),
        ("A2", "3"),
        ("A3", "=A1*A2+1"),
        ("B1", "rate"),
        ("C1", "={1;2;3}"),
        ("D1", "TRUE"),
    ] {
        assert!(
            book.enter(at(sheet1, address), text).accepted(),
            "{address}"
        );
    }
    let (bytes, _) = book.save().expect("saved");
    if let Ok(dir) = std::env::var("DNACALC_RIF_SAVE_DIR") {
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(std::path::Path::new(&dir).join("dc_rif_book1.xlsx"), &bytes).unwrap();
    }
}

/// Native-only: it writes the saved workbooks to the folder `DNACALC_RIF_SAVE_DIR` names, for
/// Excel to open (calc-host's `tests/fixtures/d2/measure-saved-open.ps1`): a new workbook edited
/// by a fill down and a copy and paste, calc-host's `dc_09v/formats.xlsx` with formatted cells
/// edited, whose formats a save keeps, and the same workbook with a row and a column inserted
/// and a formatted cell cut and pasted, whose formats and sizes move with their cells (dc-09v).
/// Without the variable it only saves and reopens.
#[test]
fn dc_09v__edited_workbooks_save_for_excel() {
    let mut book = Workbook::new_blank();
    let sheet1 = sheet(&book, "Sheet1");
    for (address, text) in [("A1", "1"), ("A2", "2"), ("A3", "3"), ("B1", "=A1*10")] {
        assert!(
            book.enter(at(sheet1, address), text).accepted(),
            "{address}"
        );
    }
    let area = |s: SheetId, a: &str, b: &str| GridArea::new(at(s, a), at(s, b)).unwrap();
    let fill = book.dispatch(SkinIntent::Copy {
        from: area(sheet1, "B1", "B1"),
        to: area(sheet1, "B2", "B3"),
        cut: false,
    });
    assert!(fill.accepted(), "{:?}", fill.error);
    let paste = book.dispatch(SkinIntent::Copy {
        from: area(sheet1, "A1", "B3"),
        to: area(sheet1, "D5", "D5"),
        cut: false,
    });
    assert!(paste.accepted(), "{:?}", paste.error);
    assert_eq!(shown(&book, at(sheet1, "E7")).text, "30");
    let (copied, _) = book.save().expect("saved");

    let mut formats = Workbook::open(
        "formats.xlsx",
        include_bytes!("../../../crates/calc-host/tests/fixtures/dc_09v/formats.xlsx").to_vec(),
    )
    .expect("it opens");
    let f = sheet(&formats, "Formats");
    assert!(formats.enter(at(f, "A1"), "9876.5").accepted());
    assert!(formats.enter(at(f, "A2"), "0.125").accepted());
    assert_eq!(shown(&formats, at(f, "A1")).text, "9,876.50");
    assert_eq!(shown(&formats, at(f, "A2")).text, "13%");
    let (edited, _) = formats.save().expect("saved");

    // A row inserted at 2 and a column before B: each inserted band takes the size of the one
    // before it, the cells below and to the right move with their formats and sizes; then the
    // formatted A3 (25%, the file's A2) is cut and pasted to H3.
    let open_formats = || {
        Workbook::open(
            "formats.xlsx",
            include_bytes!("../../../crates/calc-host/tests/fixtures/dc_09v/formats.xlsx").to_vec(),
        )
        .expect("it opens")
    };
    let mut axis = open_formats();
    let s = sheet(&axis, "Formats");
    for (kind, first) in [(calc_host::Axis::Rows, 2), (calc_host::Axis::Columns, 2)] {
        let edit = calc_host::AxisEdit::new(s, kind, calc_host::AxisChange::Insert, first, 1)
            .expect("an axis edit");
        let receipt = axis.dispatch(SkinIntent::EditAxis(edit));
        assert!(receipt.accepted(), "{:?}", receipt.error);
    }
    let cut = axis.dispatch(SkinIntent::Copy {
        from: area(s, "A3", "A3"),
        to: area(s, "I3", "I3"),
        cut: true,
    });
    assert!(cut.accepted(), "{:?}", cut.error);
    let check = |book: &Workbook, label: &str| {
        let s = sheet(book, "Formats");
        assert_eq!(shown(book, at(s, "I3")).text, "25%", "{label}");
        assert_eq!(shown(book, at(s, "A4")).text, "2024-01-31", "{label}");
        assert_eq!(shown(book, at(s, "A2")).text, "", "{label}");
        assert_eq!(shown(book, at(s, "C1")).text, "wide", "{label}");
        let geometry = book.host().sheet_geometry(s).expect("the geometry");
        let width = |column: u32| geometry.column(column).and_then(|span| span.width);
        // B, inserted, takes A's width; the file's B (25.71) is now C, its hidden C now D.
        assert!(
            width(2).is_some_and(|w| w > 16.0 && w < 17.0),
            "{label}: {geometry:?}"
        );
        assert!(
            width(3).is_some_and(|w| w > 25.0 && w < 26.0),
            "{label}: {geometry:?}"
        );
        assert!(
            geometry.column(4).is_some_and(|span| span.hidden),
            "{label}"
        );
        assert_eq!(
            geometry.row(4).and_then(|row| row.height),
            Some(30.0),
            "{label}"
        );
        assert!(geometry.row(14).is_some_and(|row| row.hidden), "{label}");
    };
    check(&axis, "the session");
    let (axis_saved, _) = axis.save().expect("saved after the insertions");
    check(
        &Workbook::open("formats.xlsx", axis_saved.clone()).expect("the saved package opens"),
        "the saved package",
    );

    if let Ok(dir) = std::env::var("DNACALC_RIF_SAVE_DIR") {
        std::fs::create_dir_all(&dir).unwrap();
        let dir = std::path::Path::new(&dir);
        std::fs::write(dir.join("dc_09v_copied.xlsx"), &copied).unwrap();
        std::fs::write(dir.join("dc_09v_formats_edited.xlsx"), &edited).unwrap();
        std::fs::write(dir.join("dc_09v_axis_edited.xlsx"), &axis_saved).unwrap();
    }
}

/// Native-only: it reads the workbook the environment variable `DNACALC_OPEN_TIMING` names from
/// the file system and prints how long DNA Calc's open took, read and calculated apart (dc-09v
/// compares it with the browser's); without the variable it does nothing.
#[test]
fn dc_09v__open_timing() {
    let Ok(path) = std::env::var("DNACALC_OPEN_TIMING") else {
        return;
    };
    let bytes = std::fs::read(&path).expect("the workbook");
    let started = dna_calc::controller::now_ms();
    let book = Workbook::open("timed.xlsx", bytes).expect("it opens");
    let total = dna_calc::controller::now_ms() - started;
    let calculated = book.calculated_on_open().unwrap_or(0.0);
    println!(
        "DC09V native open: {total:.0} ms: read {:.0} ms, calculated {calculated:.0} ms",
        total - calculated
    );
}

/// dc-09v (D4b): cells show their number formats through calc-func's one renderer. The expected
/// texts are those Excel 16.0.20430 showed for calc-host's `tests/fixtures/dc_09v/formats.xlsx`
/// (`excel-shown.json`, Range.Text), except A8, whose AM/PM designator Excel took from this
/// machine's locale (`pm`); DNA Calc shows numbers under en-US, whose designator is `PM`.
#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn dc_09v__cells_show_their_number_formats() {
    let book = Workbook::open(
        "formats.xlsx",
        include_bytes!("../../../crates/calc-host/tests/fixtures/dc_09v/formats.xlsx").to_vec(),
    )
    .expect("it opens");
    let formats = sheet(&book, "Formats");
    for (address, text) in [
        ("A1", "1,234.50"),
        ("A2", "25%"),
        ("A3", "2024-01-31"),
        ("A4", "(5.00)"),
        ("A5", "text"),
        ("A6", "1234,567"),
        ("A7", "2469.0"),
        ("A8", "12:00 PM"),
        ("A9", "3.142"),
        ("A10", "1.50E+06"),
        ("A11", "Qty 12"),
        ("A12", "0.333333333"),
    ] {
        let cell = shown(&book, at(formats, address));
        assert_eq!(cell.text, text, "{address}");
        let expected = if address == "A12" {
            cells::FormatShown::General
        } else {
            cells::FormatShown::Applied
        };
        assert_eq!(cell.format, expected, "{address}");
    }
    // The negative section of A4's code ([Red] in its second section) is red; A1's names none.
    // Palette colors ([Color10], [Color45], [Color53]) as Excel showed them (excel-shown.json).
    for (address, color) in [
        ("A4", "#ff0000"),
        ("A15", "#008000"),
        ("A16", "#ff9900"),
        ("A17", "#993300"),
    ] {
        assert_eq!(
            shown(&book, at(formats, address)).color.as_deref(),
            Some(color),
            "{address}"
        );
    }
    assert_eq!(shown(&book, at(formats, "A1")).color, None);
}

#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn dc_971p_7__sheet_lifecycle_worker_history_references_and_saved_package() {
    use dna_calc_worker::{
        server::{Server, answer_text},
        wire::{self, FromServer, Intent, Key, Reply, Request, Sheet, SheetChange as C, ToServer},
    };
    fn send(server: &mut Server, intent: Intent) -> wire::Outcome {
        let text = wire::encode(&ToServer {
            id: 1,
            request: server.envelope(Request::Intent(intent)),
        });
        let (answer, _) = answer_text(server, &text, None);
        let FromServer::Reply {
            reply: Reply::Done { outcome, .. },
            ..
        } = wire::decode(&answer).unwrap()
        else {
            panic!("{answer}")
        };
        outcome
    }
    let mut server = Server::new();
    let first = server.book().sheets()[0].id;
    let first_key = server.book().host().sheets()[0].key();
    let edit = |change| Intent::EditSheet {
        sheet: Some(Sheet::of(first)),
        change,
    };
    assert!(!send(&mut server, edit(C::Hide)).accepted);
    assert!(!send(&mut server, edit(C::Remove)).accepted);
    assert!(
        send(
            &mut server,
            Intent::EnterCell {
                at: Key::of(CellKey::new(first, 1, 1).unwrap()),
                text: "7".into()
            }
        )
        .accepted
    );
    assert!(
        send(
            &mut server,
            edit(C::Add {
                name: "Inputs".into()
            })
        )
        .accepted
    );
    let second = server.book().sheets()[1].id;
    let key = CellKey::new(second, 1, 1).unwrap();
    assert!(
        send(
            &mut server,
            Intent::EnterCell {
                at: Key::of(key),
                text: "=Sheet1!A1*2".into()
            }
        )
        .accepted
    );
    assert_eq!(shown(server.book(), key).text, "14");
    assert!(
        send(
            &mut server,
            edit(C::Rename {
                name: "October's inputs".into()
            })
        )
        .accepted
    );
    assert_eq!(
        cells::input_text(server.book().host().model().cell(key)),
        "='October''s inputs'!A1*2"
    );
    let renamed = server.book().host().model().sheets()[0].name().clone();
    assert!(
        !send(
            &mut server,
            edit(C::Rename {
                name: "inputs".into()
            })
        )
        .accepted
    );
    assert_eq!(server.book().host().model().sheets()[0].name(), &renamed);
    assert!(send(&mut server, edit(C::Move { position: 2 })).accepted);
    assert!(send(&mut server, edit(C::Hide)).accepted);
    let (bytes, _) = server.book_mut().save().unwrap();
    let reopened = Workbook::open("sheet-lifecycle.xlsx", bytes).unwrap();
    assert_eq!(
        reopened
            .sheets()
            .iter()
            .map(|s| s.name.as_str())
            .collect::<Vec<_>>(),
        vec!["Inputs", "October's inputs"]
    );
    assert_eq!(
        reopened.host().sheets()[1].visibility(),
        calc_host::SheetVisibility::Hidden
    );
    let saved_key = CellKey::new(reopened.sheets()[0].id, 1, 1).unwrap();
    assert_eq!(shown(&reopened, saved_key).text, "14");
    assert!(send(&mut server, edit(C::Remove)).accepted);
    assert_eq!(shown(server.book(), key).text, "#REF!");
    assert!(send(&mut server, Intent::Undo).accepted);
    assert_eq!(shown(server.book(), key).text, "14");
    assert!(send(&mut server, Intent::Redo).accepted);
    assert_eq!(shown(server.book(), key).text, "#REF!");
    let (bytes, _) = server.book_mut().save().unwrap();
    let reopened = Workbook::open("removed-sheet.xlsx", bytes).unwrap();
    assert_eq!(reopened.sheets().len(), 1);
    assert_eq!(
        shown(
            &reopened,
            CellKey::new(reopened.sheets()[0].id, 1, 1).unwrap()
        )
        .text,
        "#REF!"
    );
    assert!(
        send(
            &mut server,
            Intent::EditSheet {
                sheet: None,
                change: C::Add {
                    name: "Fresh".into()
                }
            }
        )
        .accepted
    );
    let fresh = server
        .book()
        .host()
        .sheets()
        .iter()
        .find(|s| s.name().text().to_string_lossy() == "Fresh")
        .unwrap();
    let fresh_id = fresh.id();
    assert_ne!(fresh_id, first);
    assert_ne!(fresh.key(), first_key);
    assert!(send(&mut server, Intent::Undo).accepted);
    assert!(send(&mut server, Intent::Redo).accepted);
    assert_eq!(
        server
            .book()
            .host()
            .sheets()
            .iter()
            .find(|s| s.name().text().to_string_lossy() == "Fresh")
            .unwrap()
            .id(),
        fresh_id
    );
}

#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn dc_971p_7__cell_shifts_rewrite_sources_with_one_history_step() {
    use dna_calc_worker::{
        server::{Server, answer_text},
        wire::{self, Area, FromServer, Intent, Key, Reply, Request, ToServer},
    };
    fn send(server: &mut Server, intent: Intent) -> wire::Outcome {
        let (answer, _) = answer_text(
            server,
            &wire::encode(&ToServer {
                id: 1,
                request: server.envelope(Request::Intent(intent)),
            }),
            None,
        );
        let FromServer::Reply {
            reply: Reply::Done { outcome, .. },
            ..
        } = wire::decode(&answer).unwrap()
        else {
            panic!("{answer}")
        };
        outcome
    }
    let mut server = Server::new();
    let sheet = server.book().sheets()[0].id;
    let at = |r, c| CellKey::new(sheet, r, c).unwrap();
    for (r, c, text) in [
        (1, 1, "1"),
        (2, 1, "2"),
        (3, 1, "3"),
        (1, 2, "=SUM(A1:A3)"),
        (2, 2, "=$A$2"),
        (3, 2, "=A3*10"),
    ] {
        assert!(
            send(
                &mut server,
                Intent::EnterCell {
                    at: Key::of(at(r, c)),
                    text: text.into()
                }
            )
            .accepted
        );
    }
    let area = Area::of(GridArea::cell(at(2, 1)));
    let before = server.book().session().history_depth().0;
    let insert = send(
        &mut server,
        Intent::ShiftCells {
            area,
            down: true,
            insert: true,
        },
    );
    assert!(insert.accepted, "{:?}", insert.error);
    assert_eq!(server.book().session().history_depth().0, before + 1);
    assert_eq!(shown(server.book(), at(2, 1)).text, "");
    assert_eq!(shown(server.book(), at(3, 1)).text, "2");
    assert_eq!(shown(server.book(), at(4, 1)).text, "3");
    assert_eq!(
        cells::input_text(server.book().host().model().cell(at(1, 2))),
        "=SUM(A1:A4)"
    );
    assert_eq!(
        cells::input_text(server.book().host().model().cell(at(2, 2))),
        "=$A$3"
    );
    assert!(send(&mut server, Intent::Undo).accepted);
    assert_eq!(shown(server.book(), at(3, 1)).text, "3");
    assert!(send(&mut server, Intent::Redo).accepted);
    let delete = send(
        &mut server,
        Intent::ShiftCells {
            area: Area::of(GridArea::cell(at(3, 1))),
            down: true,
            insert: false,
        },
    );
    assert!(delete.accepted, "{:?}", delete.error);
    assert_eq!(shown(server.book(), at(2, 2)).text, "#REF!");
    assert_eq!(shown(server.book(), at(3, 1)).text, "3");
    let (bytes, _) = server.book_mut().save().unwrap();
    let reopened = Workbook::open("shift.xlsx", bytes).unwrap();
    let s = reopened.sheets()[0].id;
    assert_eq!(shown(&reopened, CellKey::new(s, 3, 1).unwrap()).text, "3");
    assert_eq!(
        shown(&reopened, CellKey::new(s, 2, 2).unwrap()).text,
        "#REF!"
    );
    // A partially crossing range is refused atomically until its Excel behaviour is measured.
    assert!(
        send(
            &mut server,
            Intent::EnterCell {
                at: Key::of(at(1, 3)),
                text: "=SUM(A1:B4)".into()
            }
        )
        .accepted
    );
    let revision = server.book().host().revision();
    let text = wire::encode(&ToServer {
        id: 2,
        request: server.envelope(Request::Intent(Intent::ShiftCells {
            area,
            down: true,
            insert: true,
        })),
    });
    let (answer, _) = answer_text(&mut server, &text, None);
    let FromServer::Failed { message, .. } = wire::decode(&answer).unwrap() else {
        panic!("{answer}")
    };
    assert_eq!(server.book().host().revision(), revision);
    assert!(message.contains("crossing ranges"));
}

#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn dc_971p_7__shift_keeps_imported_styles_and_absolute_defined_names() {
    let mut book = Workbook::open(
        "formats.xlsx",
        include_bytes!("../../../crates/calc-host/tests/fixtures/dc_09v/formats.xlsx").to_vec(),
    )
    .unwrap();
    let sheet = book.sheets()[0].id;
    let at = |r, c| CellKey::new(sheet, r, c).unwrap();
    let formula_value = shown(&book, at(7, 1)).text;
    let style = book.host().cell_style(at(1, 1));
    assert_ne!(style, 0);
    assert!(
        book.dispatch(SkinIntent::Edit {
            edits: vec![calc_host::CellEdit::SetName {
                name: "Anchor".into(),
                scope: calc_host::NameScope::Workbook,
                formula: Some("Formats!$A$1".into())
            }]
        })
        .accepted()
    );
    let edits =
        calc_host::shift_cell_edits(book.host(), GridArea::cell(at(1, 1)), false, true).unwrap();
    assert!(book.dispatch(SkinIntent::Edit { edits }).accepted());
    assert_eq!(book.host().cell_style(at(1, 2)), style);
    assert_eq!(book.host().cell_style(at(1, 1)), 0);
    assert_eq!(
        book.host()
            .model()
            .definitions()
            .names()
            .iter()
            .find(|n| n.name == "Anchor")
            .unwrap()
            .formula,
        "Formats!$B$1"
    );
    assert_eq!(shown(&book, at(7, 1)).text, formula_value);
    assert_eq!(
        cells::input_text(book.host().model().cell(at(7, 1))),
        "=B1*2"
    );
    let (bytes, _) = book.save().unwrap();
    let reopened = Workbook::open("styles-shift.xlsx", bytes).unwrap();
    let saved = CellKey::new(reopened.sheets()[0].id, 1, 2).unwrap();
    assert_eq!(reopened.host().cell_style(saved), style);
    assert_eq!(shown(&reopened, saved).text, "1,234.50");
    assert!(book.dispatch(SkinIntent::Undo).accepted());
    assert_eq!(book.host().cell_style(at(1, 1)), style);
    assert_eq!(
        book.host()
            .model()
            .definitions()
            .names()
            .iter()
            .find(|n| n.name == "Anchor")
            .unwrap()
            .formula,
        "Formats!$A$1"
    );
}

#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn dc_971p_7__saved_sheet_names_preserve_xstring_text_and_attribute_whitespace() {
    let mut book = Workbook::new_blank();
    let sheet = book.sheets()[0].id;
    for text in ["literal _x0041_", "a&b\"quoted", "tab\tand\nnewline"] {
        let name = calc_host::SheetName::new(calc_host::value::ExcelText::from_utf16_code_units(
            text.encode_utf16().collect(),
        ))
        .unwrap();
        assert!(
            book.dispatch(SkinIntent::Edit {
                edits: vec![calc_host::CellEdit::Sheet(calc_host::SheetEdit::Rename {
                    sheet,
                    name
                })]
            })
            .accepted()
        );
        let (bytes, _) = book.save().unwrap();
        let reopened = Workbook::open("names.xlsx", bytes).unwrap();
        assert_eq!(reopened.sheets()[0].name, text);
    }
}

#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn dc_971p_7__partial_spills_and_relative_names_refuse_before_mutation() {
    let mut book = Workbook::new_blank();
    let sheet = book.sheets()[0].id;
    let at = |r, c| CellKey::new(sheet, r, c).unwrap();
    assert!(
        book.dispatch(SkinIntent::EnterCell {
            at: at(1, 1),
            text: "=SEQUENCE(3)".into()
        })
        .accepted()
    );
    assert!(
        calc_host::shift_cell_edits(book.host(), GridArea::cell(at(2, 1)), true, true).is_err()
    );
    assert!(
        book.dispatch(SkinIntent::EnterCell {
            at: at(1, 1),
            text: "1".into()
        })
        .accepted()
    );
    assert!(
        book.dispatch(SkinIntent::Edit {
            edits: vec![calc_host::CellEdit::SetName {
                name: "Relative".into(),
                scope: calc_host::NameScope::Workbook,
                formula: Some("Sheet1!B$2".into())
            }]
        })
        .accepted()
    );
    assert!(
        calc_host::shift_cell_edits(book.host(), GridArea::cell(at(1, 1)), true, true).is_err()
    );
}

#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn dc_971p_7__mixed_sheet_batches_are_refused_before_allocating_duplicate_identities() {
    let mut book = Workbook::new_blank();
    let sheet = book.sheets()[0].id;
    let name = |s: &str| {
        calc_host::SheetName::new(calc_host::value::ExcelText::from_utf16_code_units(
            s.encode_utf16().collect(),
        ))
        .unwrap()
    };
    let result = book.dispatch(SkinIntent::Edit {
        edits: vec![
            calc_host::CellEdit::Sheet(calc_host::SheetEdit::Rename {
                sheet,
                name: name("Renamed"),
            }),
            calc_host::CellEdit::Sheet(calc_host::SheetEdit::Insert {
                key: calc_host::SheetKey::new(2),
                name: name("Added"),
                position: std::num::NonZeroU32::new(2).unwrap(),
            }),
        ],
    });
    assert!(!result.accepted());
    assert_eq!(book.sheets()[0].name, "Sheet1");
    assert_eq!(book.sheets().len(), 1);
}

#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn dc_971p_7__hiding_and_unhiding_sheets_preserves_current_values_in_manual_mode() {
    let mut book = Workbook::new_blank();
    let first = book.sheets()[0].id;
    let at = CellKey::new(first, 1, 1).unwrap();
    assert!(
        book.dispatch(SkinIntent::Edit {
            edits: vec![calc_host::CellEdit::Sheet(calc_host::SheetEdit::Insert {
                key: calc_host::SheetKey::new(2),
                name: calc_host::SheetName::new(
                    calc_host::value::ExcelText::from_utf16_code_units(
                        "Other".encode_utf16().collect()
                    )
                )
                .unwrap(),
                position: std::num::NonZeroU32::new(2).unwrap()
            })]
        })
        .accepted()
    );
    let second = book.sheets()[1].id;
    assert!(
        book.dispatch(SkinIntent::EnterCell {
            at,
            text: "=2+3".into()
        })
        .accepted()
    );
    assert!(
        book.dispatch(SkinIntent::SetCalcMode(calc_host::CalcMode::Manual))
            .accepted()
    );
    assert!(
        book.dispatch(SkinIntent::Edit {
            edits: vec![calc_host::CellEdit::Sheet(
                calc_host::SheetEdit::SetVisibility {
                    sheet: second,
                    visibility: calc_host::SheetVisibility::Hidden
                }
            )]
        })
        .accepted()
    );
    assert_eq!(shown(&book, at).provenance, Provenance::Calculated);
    assert!(book.dispatch(SkinIntent::Undo).accepted());
    assert_eq!(shown(&book, at).provenance, Provenance::Calculated);
    assert_eq!(shown(&book, at).text, "5");
}

mod rule_review {
    use super::*;
    use dna_calc_worker::{
        server::Server,
        wire::{self, ConditionalChange as C, Intent, Key, Reply, Request, Sheet},
    };
    fn intent(server: &mut Server, intent: Intent) -> wire::Outcome {
        match server
            .execute_current(Request::Intent(intent), None)
            .reply
            .unwrap()
        {
            Reply::Done { outcome, .. } => outcome,
            other => panic!("{other:?}"),
        }
    }
    fn details(server: &mut Server, at: CellKey) -> wire::Review {
        match server
            .execute_current(Request::Review(Key::of(at)), None)
            .reply
            .unwrap()
        {
            Reply::Review(review) => review,
            other => panic!("{other:?}"),
        }
    }
    fn open(server: &mut Server, name: &str, bytes: &[u8]) {
        let answer =
            server.execute_current(Request::Open { name: name.into() }, Some(bytes.to_vec()));
        assert!(
            matches!(
                answer.reply,
                Ok(Reply::Done {
                    outcome: wire::Outcome { accepted: true, .. },
                    ..
                })
            ),
            "{:?}",
            answer.reply
        );
    }
    fn change(server: &mut Server, sheet: SheetId, index: Option<u32>, change: C) -> wire::Outcome {
        intent(
            server,
            Intent::EditConditional {
                sheet: Sheet::of(sheet),
                revision: server.book().host().revision().get(),
                index,
                change,
            },
        )
    }
    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn dc_971p_8__imported_rules_explain_edit_history_and_save_without_losing_other_rules() {
        let mut server = Server::new();
        open(
            &mut server,
            "conditional-formats.xlsx",
            include_bytes!(
                "../../../crates/calc-host/tests/fixtures/dc_6yw/conditional-formats.xlsx"
            ),
        );
        let sheet = server.book().sheets()[0].id;
        let a2 = CellKey::new(sheet, 2, 1).unwrap();
        let review = details(&mut server, a2);
        assert_eq!(review.rules.len(), 15);
        let before = server
            .book()
            .host()
            .model()
            .definitions()
            .conditional_rules(sheet)
            .to_vec();
        assert!(
            intent(
                &mut server,
                Intent::FreezePanes {
                    sheet: Sheet::of(sheet),
                    rows: 1,
                    columns: 1
                }
            )
            .accepted
        );
        assert!(
            intent(
                &mut server,
                Intent::FreezePanes {
                    sheet: Sheet::of(sheet),
                    rows: 0,
                    columns: 0
                }
            )
            .accepted
        );
        assert_eq!(
            server
                .book()
                .host()
                .model()
                .definitions()
                .conditional_rules(sheet),
            before
        );
        assert!(intent(&mut server, Intent::Undo).accepted);
        assert_eq!(
            server
                .book()
                .host()
                .model()
                .definitions()
                .conditional_rules(sheet),
            before
        );

        assert!(
            server
                .execute_current(
                    Request::Subscribe {
                        id: 42,
                        interest: wire::Interest::Area(wire::Area::of(
                            GridArea::new(
                                CellKey::new(sheet, 1, 1).unwrap(),
                                CellKey::new(sheet, 10, 1).unwrap()
                            )
                            .unwrap()
                        ))
                    },
                    None
                )
                .reply
                .is_ok()
        );
        for event in server.take_events() {
            if let wire::Event::TileCompletion(barrier) = event {
                assert!(server.handle(Request::TileAck(barrier), None).reply.is_ok());
            }
        }
        let rule = review.rules.iter().find(|r| r.applies == "A1:A10").unwrap();
        assert_eq!(rule.state, "Applies appearance to this cell");
        let index = rule.index;
        let refused = server.book().host().conditional_formats(sheet);
        let count = refused.refused();
        refused.explain(server.book().host(), a2);
        assert_eq!(refused.refused(), count);
        assert!(
            change(
                &mut server,
                sheet,
                Some(index),
                C::Update {
                    applies: "A1:A10".into(),
                    formulas: vec!["50".into()],
                    stop: true
                }
            )
            .accepted
        );
        assert_eq!(
            shown(server.book(), a2).conditional.and_then(|c| c.fill),
            None
        );
        let events = server.take_events();
        assert!(
            events.iter().any(|event| {
                let wire::Event::Tiles(cells) = event else {
                    return false;
                };
                cells.stamp.reset
                    && cells
                        .cells
                        .iter()
                        .any(|c| c.at == Key::of(a2) && c.conditional.is_none())
            }),
            "unchanged inputs must receive updated rule appearance"
        );
        assert_eq!(
            details(&mut server, a2)
                .rules
                .iter()
                .find(|r| r.index == index)
                .unwrap()
                .state,
            "Condition false for this cell"
        );
        assert!(intent(&mut server, Intent::Undo).accepted);
        assert_eq!(
            shown(server.book(), a2)
                .conditional
                .and_then(|c| c.fill)
                .as_deref(),
            Some("#ffc7ce")
        );
        assert!(intent(&mut server, Intent::Redo).accepted);
        let (bytes, _) = server.book_mut().save().unwrap();
        let reopened = Workbook::open("changed-rules.xlsx", bytes).unwrap();
        let saved_sheet = reopened.sheets()[0].id;
        assert_eq!(
            reopened
                .host()
                .model()
                .definitions()
                .conditional_rules(saved_sheet)
                .len(),
            15
        );
        assert_eq!(
            shown(&reopened, CellKey::new(saved_sheet, 2, 1).unwrap())
                .conditional
                .and_then(|c| c.fill),
            None
        );
        assert_eq!(
            shown(&reopened, CellKey::new(saved_sheet, 6, 7).unwrap())
                .conditional
                .unwrap()
                .bar
                .unwrap()
                .0,
            1.0
        );
        for column in 2..=14 {
            for row in 1..=10 {
                let live = shown(server.book(), CellKey::new(sheet, row, column).unwrap());
                let saved = shown(&reopened, CellKey::new(saved_sheet, row, column).unwrap());
                assert_eq!(
                    live.text, saved.text,
                    "saved rule number format at {row},{column}"
                );
                assert_eq!(
                    live.conditional, saved.conditional,
                    "saved rule appearance at {row},{column}"
                );
            }
        }
        let index = details(&mut server, a2)
            .rules
            .into_iter()
            .find(|r| r.kind == "dataBar")
            .unwrap()
            .index;
        assert!(
            !change(&mut server, sheet, Some(index), C::Remove).accepted,
            "extension-backed rules are preserved"
        );
        assert!(
            change(
                &mut server,
                sheet,
                Some(2),
                C::Update {
                    applies: "P1".into(),
                    formulas: vec!["TRUE".into()],
                    stop: true
                }
            )
            .accepted
        );
        let blank = CellKey::new(sheet, 1, 16).unwrap();
        assert!(server.book().host().model().cell(blank).is_none());
        assert!(shown(server.book(), blank).conditional.is_some());
        assert_eq!(
            details(&mut server, blank)
                .rules
                .iter()
                .find(|r| r.index == 2)
                .unwrap()
                .state,
            "Applies appearance to this cell"
        );
    }
    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn dc_971p_8__authored_rule_ranges_priorities_removal_and_save_use_host_history() {
        let mut server = Server::new();
        let sheet = server.book().sheets()[0].id;
        let a = |row| CellKey::new(sheet, row, 1).unwrap();
        for (row, text) in [(1, "1"), (2, "5"), (3, "10")] {
            assert!(
                intent(
                    &mut server,
                    Intent::EnterCell {
                        at: Key::of(a(row)),
                        text: text.into()
                    }
                )
                .accepted
            );
        }
        for kind in ["colorScale", "dataBar", "iconSet"] {
            assert!(
                change(
                    &mut server,
                    sheet,
                    None,
                    C::Add {
                        kind: kind.into(),
                        applies: "A1:A3".into(),
                        color: "327A94".into()
                    }
                )
                .accepted
            );
        }
        let review = details(&mut server, a(2));
        assert_eq!(review.rules.len(), 3);
        assert!(change(&mut server, sheet, Some(2), C::Move { up: true }).accepted);
        assert_eq!(
            details(&mut server, a(2))
                .rules
                .iter()
                .map(|r| r.kind.as_str())
                .collect::<Vec<_>>(),
            vec!["colorScale", "iconSet", "dataBar"]
        );
        assert!(
            change(
                &mut server,
                sheet,
                Some(0),
                C::Update {
                    applies: "A1:A2".into(),
                    formulas: vec![],
                    stop: false
                }
            )
            .accepted
        );
        assert_eq!(
            details(&mut server, a(3)).rules[0].state,
            "Outside this cell"
        );
        assert!(change(&mut server, sheet, Some(1), C::Remove).accepted);
        assert!(intent(&mut server, Intent::Undo).accepted);
        let before = server.book().host().revision().get();
        assert!(
            !change(
                &mut server,
                sheet,
                None,
                C::Add {
                    kind: "dataBar".into(),
                    applies: "not a range".into(),
                    color: "327A94".into()
                }
            )
            .accepted
        );
        assert_eq!(server.book().host().revision().get(), before);
        let (bytes, _) = server.book_mut().save().unwrap();
        let reopened = Workbook::open("new-rules.xlsx", bytes).unwrap();
        let s = reopened.sheets()[0].id;
        assert_eq!(
            reopened
                .host()
                .model()
                .definitions()
                .conditional_rules(s)
                .len(),
            3
        );
        assert!(
            shown(&reopened, CellKey::new(s, 2, 1).unwrap())
                .conditional
                .unwrap()
                .icon
                .is_some()
        );
        assert_eq!(
            shown(&reopened, CellKey::new(s, 3, 1).unwrap())
                .conditional
                .unwrap()
                .bar
                .unwrap()
                .0,
            1.0
        );
        // Removing all rules must also remove all serialized standard blocks.
        while !details(&mut server, a(1)).rules.is_empty() {
            assert!(change(&mut server, sheet, Some(0), C::Remove).accepted);
        }
        let (bytes, _) = server.book_mut().save().unwrap();
        let reopened = Workbook::open("no-rules.xlsx", bytes).unwrap();
        assert_eq!(
            reopened
                .host()
                .conditional_formats(reopened.sheets()[0].id)
                .rule_count(),
            0
        );
    }
    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn dc_971p_8__validation_waits_for_an_explicit_current_decision() {
        let mut server = Server::new();
        open(
            &mut server,
            "data-validation.xlsx",
            include_bytes!("../../../crates/calc-host/tests/fixtures/dc_6yw/data-validation.xlsx"),
        );
        let sheet = server.book().sheets()[0].id;
        let at = |r, c| CellKey::new(sheet, r, c).unwrap();
        let warning = at(1, 4);
        let before = shown(server.book(), warning).text;
        let outcome = intent(
            &mut server,
            Intent::EnterCell {
                at: Key::of(warning),
                text: "-1".into(),
            },
        );
        assert!(!outcome.accepted && !outcome.committed);
        let decision = outcome.validation.unwrap();
        assert_eq!(decision.style, "Warning");
        assert_eq!(shown(server.book(), warning).text, before);
        assert!(
            intent(
                &mut server,
                Intent::AcceptValidation {
                    token: decision.token
                }
            )
            .accepted
        );
        assert_eq!(shown(server.book(), warning).text, "-1");
        assert!(intent(&mut server, Intent::Undo).accepted);
        assert_eq!(shown(server.book(), warning).text, before);
        let outcome = intent(
            &mut server,
            Intent::EnterCell {
                at: Key::of(at(1, 5)),
                text: "toolong".into(),
            },
        );
        assert_eq!(outcome.validation.as_ref().unwrap().style, "Information");
        assert!(!outcome.committed);
        let token = outcome.validation.unwrap().token;
        assert!(
            intent(
                &mut server,
                Intent::EnterCell {
                    at: Key::of(at(8, 8)),
                    text: "2".into()
                }
            )
            .accepted
        );
        assert!(
            server
                .execute_current(Request::Intent(Intent::AcceptValidation { token }), None)
                .reply
                .is_err(),
            "a later entry expires the pending decision"
        );
        let stop = intent(
            &mut server,
            Intent::EnterCell {
                at: Key::of(at(2, 1)),
                text: "Purple".into(),
            },
        );
        assert_eq!(stop.validation.unwrap().style, "Stop");
        assert!(!stop.accepted);
    }
    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn dc_971p_8__notes_and_links_follow_axis_edits_and_round_trip() {
        let mut server = Server::new();
        open(
            &mut server,
            "features.xlsx",
            include_bytes!("../../../crates/calc-doc/tests/fixtures/dc_5sr/features.xlsx"),
        );
        let sheet = server.book().sheets()[0].id;
        let at = |r, c| CellKey::new(sheet, r, c).unwrap();
        let review = details(&mut server, at(1, 1));
        assert_eq!(review.notes.len(), 1);
        assert_eq!(review.notes[0].1, "a comment");
        let link = details(&mut server, at(1, 11)).links.remove(0);
        assert_eq!(link.tooltip, "a tip");
        assert!(link.target.as_deref().unwrap().starts_with("https://"));
        assert!(
            intent(
                &mut server,
                Intent::EditAxis {
                    sheet: Sheet::of(sheet),
                    rows: true,
                    insert: true,
                    start: 1,
                    count: 1
                }
            )
            .accepted
        );
        assert!(details(&mut server, at(1, 1)).notes.is_empty());
        assert_eq!(details(&mut server, at(2, 1)).notes[0].1, "a comment");
        assert_eq!(details(&mut server, at(2, 11)).links.len(), 1);
        let (bytes, _) = server.book_mut().save().unwrap();
        let reopened = Workbook::open("review.xlsx", bytes).unwrap();
        let s = reopened.sheets()[0].id;
        let review = reopened.host().sheet_review(s);
        assert_eq!(review.notes[0].at.row(), 2);
        assert_eq!(review.links[0].area.first().row(), 2);
    }
}

mod workbook_commands {
    use super::*;
    use dna_calc_worker::{
        server::Server,
        wire::{self, Area, FindOptions, FindScope, Intent, Key, Reply, Request, Sheet},
    };
    fn request(server: &mut Server, request: Request) -> Reply {
        server.execute_current(request, None).reply.unwrap()
    }
    fn send(server: &mut Server, intent: Intent) -> wire::Outcome {
        match request(server, Request::Intent(intent)) {
            Reply::Done { outcome, .. } => outcome,
            other => panic!("{other:?}"),
        }
    }
    fn stats(server: &mut Server) -> wire::SelectionStats {
        match request(server, Request::SelectionStats) {
            Reply::SelectionStats(Ok(stats)) => stats,
            other => panic!("{other:?}"),
        }
    }
    fn found(server: &mut Server, options: FindOptions) -> wire::Found {
        match request(server, Request::Find(options)) {
            Reply::Found(Ok(found)) => found,
            other => panic!("{other:?}"),
        }
    }
    fn options(sheet: SheetId, area: GridArea, text: &str) -> FindOptions {
        FindOptions {
            sheet: Sheet::of(sheet),
            areas: vec![Area::of(area)],
            text: text.into(),
            case_sensitive: false,
            whole: false,
            wildcards: false,
            formulas: true,
            scope: FindScope::Selection,
        }
    }
    fn set(server: &mut Server, at: CellKey, text: &str) {
        let out = send(
            server,
            Intent::EnterCell {
                at: Key::of(at),
                text: text.into(),
            },
        );
        assert!(out.accepted, "{:?}", out.error);
    }
    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn dc_971p_9__search_replace_grouped_history_and_stale_results_use_worker_messages() {
        let mut server = Server::new();
        let sheet = server.book().sheets()[0].id;
        let at = |r, c| CellKey::new(sheet, r, c).unwrap();
        let selected = GridArea::new(at(1, 1), at(3, 2)).unwrap();
        set(&mut server, at(1, 1), "1");
        set(&mut server, at(2, 1), "2");
        set(&mut server, at(1, 2), "=SUM(A1:A2)");
        set(&mut server, at(2, 2), "=SUM(A1:A2)");
        let o = options(sheet, selected, "sum(");
        let first = found(&mut server, o.clone());
        assert_eq!(first.rows.len(), 2);
        assert!(
            first
                .rows
                .iter()
                .all(|r| r.provenance == Provenance::Calculated)
        );
        let mut displayed = options(sheet, selected, "3");
        displayed.formulas = false;
        displayed.whole = true;
        assert_eq!(found(&mut server, displayed.clone()).rows.len(), 2);
        assert!(
            found(
                &mut server,
                FindOptions {
                    formulas: true,
                    ..displayed
                }
            )
            .rows
            .is_empty()
        );
        let out = send(
            &mut server,
            Intent::Replace {
                options: o.clone(),
                replacement: "AVERAGE(".into(),
                revision: first.revision,
                only: None,
            },
        );
        assert!(out.committed, "{:?}", out.error);
        assert_eq!(shown(server.book(), at(1, 2)).text, "1.5");
        assert_eq!(shown(server.book(), at(2, 2)).text, "1.5");
        let stale = send(
            &mut server,
            Intent::Replace {
                options: o.clone(),
                replacement: "MAX(".into(),
                revision: first.revision,
                only: None,
            },
        );
        assert!(!stale.accepted && !stale.committed);
        assert!(send(&mut server, Intent::Undo).accepted);
        assert_eq!(shown(server.book(), at(1, 2)).text, "3");
        assert_eq!(shown(server.book(), at(2, 2)).text, "3");
        let hits = found(&mut server, o.clone());
        let out = send(
            &mut server,
            Intent::Replace {
                options: o.clone(),
                replacement: "MAX(".into(),
                revision: hits.revision,
                only: Some(Key::of(at(2, 2))),
            },
        );
        assert!(out.committed);
        assert_eq!(shown(server.book(), at(1, 2)).text, "3");
        assert_eq!(shown(server.book(), at(2, 2)).text, "2");
        let hits = found(&mut server, o.clone());
        let refused = send(
            &mut server,
            Intent::Replace {
                options: FindOptions {
                    wildcards: true,
                    ..o
                },
                replacement: "MIN(".into(),
                revision: hits.revision,
                only: None,
            },
        );
        assert!(!refused.accepted && !refused.committed);
        let answer = server.execute_current(Request::Save, None);
        assert!(matches!(
            answer.reply,
            Ok(Reply::Done { saved: Some(_), .. })
        ));
        let saved = answer.bytes.unwrap();
        assert!(matches!(
            server
                .execute_current(
                    Request::Open {
                        name: "replaced.xlsx".into()
                    },
                    Some(saved)
                )
                .reply,
            Ok(Reply::Done {
                outcome: wire::Outcome { accepted: true, .. },
                ..
            })
        ));
        let sheet = server.book().sheets()[0].id;
        assert_eq!(
            shown(server.book(), CellKey::new(sheet, 2, 2).unwrap()).text,
            "2"
        );
    }
    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn dc_971p_9__selection_arithmetic_deduplicates_and_separates_provenance() {
        let mut server = Server::new();
        let sheet = server.book().sheets()[0].id;
        let at = |r, c| CellKey::new(sheet, r, c).unwrap();
        for (r, c, text) in [
            (1, 1, "1"),
            (2, 1, "2"),
            (3, 1, "text"),
            (4, 1, "TRUE"),
            (1, 2, "=SUM(A1:A2)"),
            (2, 2, "=1/0"),
            (3, 2, "=WEBSERVICE(\"https://example.com\")"),
            (4, 2, "=\"\""),
        ] {
            set(&mut server, at(r, c), text);
        }
        let area = GridArea::new(at(1, 1), at(5, 2)).unwrap();
        assert!(
            send(
                &mut server,
                Intent::Select {
                    active: Key::of(at(1, 1)),
                    areas: vec![Area::of(area), Area::of(GridArea::cell(at(1, 1)))]
                }
            )
            .accepted
        );
        let s = stats(&mut server);
        assert_eq!(
            (
                s.selected,
                s.count,
                s.numeric,
                s.errors,
                s.blank,
                s.empty_text,
                s.refused
            ),
            (10, 7, 3, 1, 2, 1, 1)
        );
        assert_eq!(
            (
                s.sum.as_str(),
                s.average.as_str(),
                s.min.as_str(),
                s.max.as_str()
            ),
            ("6", "2", "1", "3")
        );
        assert!(send(&mut server, Intent::SetCalcMode { manual: true }).accepted);
        set(&mut server, at(1, 1), "10");
        let s = stats(&mut server);
        assert_eq!((s.numeric, s.stale, s.refused), (2, 1, 1));
        assert_eq!(s.sum, "12");
        let whole = GridArea::new(at(1, 1), at(calc_host::value::MAX_ROWS, 1)).unwrap();
        assert!(
            send(
                &mut server,
                Intent::Select {
                    active: Key::of(at(1, 1)),
                    areas: vec![Area::of(whole)]
                }
            )
            .accepted
        );
        assert!(
            matches!(request(&mut server,Request::SelectionStats),Reply::SelectionStats(Err(message)) if message.contains("10000"))
        );
    }
    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn dc_971p_9__spill_values_and_tiny_search_scope_use_cell_projections() {
        let mut server = Server::new();
        let sheet = server.book().sheets()[0].id;
        let at = |r| CellKey::new(sheet, r, 1).unwrap();
        set(&mut server, at(1), "=SEQUENCE(100001)");
        let range = GridArea::new(at(1), at(3)).unwrap();
        assert!(
            send(
                &mut server,
                Intent::Select {
                    active: Key::of(at(1)),
                    areas: vec![Area::of(range)]
                }
            )
            .accepted
        );
        let s = stats(&mut server);
        assert_eq!((s.count, s.numeric, s.sum.as_str()), (3, 3, "6"));
        let o = FindOptions {
            formulas: false,
            whole: true,
            ..options(sheet, GridArea::cell(at(100001)), "100001")
        };
        let f = found(&mut server, o);
        assert!(!f.partial);
        assert_eq!(f.scanned, 1);
        assert_eq!(f.rows.len(), 1);
        assert_eq!(f.rows[0].at, Key::of(at(100001)));
    }
    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn dc_971p_9__workbook_search_identifies_hidden_sheets_and_validation_stops_replacement() {
        let mut server = Server::new();
        let first = server.book().sheets()[0].id;
        assert!(
            send(
                &mut server,
                Intent::EditSheet {
                    sheet: None,
                    change: wire::SheetChange::Add {
                        name: "Hidden inputs".into()
                    }
                }
            )
            .accepted
        );
        let hidden = server.book().sheets()[1].id;
        set(&mut server, CellKey::new(hidden, 1, 1).unwrap(), "café");
        assert!(
            send(
                &mut server,
                Intent::EditSheet {
                    sheet: Some(Sheet::of(hidden)),
                    change: wire::SheetChange::Hide
                }
            )
            .accepted
        );
        let o = FindOptions {
            scope: FindScope::Workbook,
            ..options(
                first,
                GridArea::cell(CellKey::new(first, 1, 1).unwrap()),
                "CAFÉ",
            )
        };
        let f = found(&mut server, o);
        assert_eq!(f.rows.len(), 1);
        assert!(f.rows[0].hidden);
        assert!(f.rows[0].label.contains("Hidden inputs"));
        let answer = server.execute_current(
            Request::Open {
                name: "validation.xlsx".into(),
            },
            Some(
                include_bytes!(
                    "../../../crates/calc-host/tests/fixtures/dc_6yw/data-validation.xlsx"
                )
                .to_vec(),
            ),
        );
        assert!(matches!(
            answer.reply,
            Ok(Reply::Done {
                outcome: wire::Outcome { accepted: true, .. },
                ..
            })
        ));
        let sheet = server.book().sheets()[0].id;
        let at = CellKey::new(sheet, 4, 4).unwrap();
        set(&mut server, at, "2");
        let o = FindOptions {
            whole: true,
            ..options(sheet, GridArea::cell(at), "2")
        };
        let f = found(&mut server, o.clone());
        let out = send(
            &mut server,
            Intent::Replace {
                options: o,
                replacement: "-1".into(),
                revision: f.revision,
                only: None,
            },
        );
        assert!(!out.accepted && !out.committed);
        assert_eq!(shown(server.book(), at).text, "2");
    }
}
#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn dc_971p_11__atlas_counts_and_paged_recovery_are_host_projections_over_messages() {
    use dna_calc_worker::{
        server::{Server, answer_text},
        wire::{self, FromServer, Intent, Key, Reply, Request, ToServer},
    };
    let mut server = Server::new();
    let Reply::Atlas(empty) = server.execute_current(Request::Atlas, None).reply.unwrap() else {
        panic!("atlas reply")
    };
    assert_eq!(empty.census.formulas(), 0);
    assert_eq!(empty.census.pending(), 0);
    let sheet = server.book().sheets()[0].id;
    for row in 1..=93 {
        let text = if row <= 60 {
            "=Lost!A1".into()
        } else if row <= 92 {
            format!("=Missing{:02}!A1", row - 61)
        } else {
            "=1+1".into()
        };
        let Reply::Done { outcome, .. } = server
            .execute_current(
                Request::Intent(Intent::EnterCell {
                    at: Key::of(CellKey::new(sheet, row, 1).unwrap()),
                    text,
                }),
                None,
            )
            .reply
            .unwrap()
        else {
            panic!("entry reply")
        };
        assert!(outcome.accepted);
    }
    let Reply::Atlas(atlas) = server.execute_current(Request::Atlas, None).reply.unwrap() else {
        panic!("atlas reply")
    };
    assert_eq!(
        (
            atlas.census.formulas(),
            atlas.census.calculated(),
            atlas.census.refused(),
            atlas.census.pending()
        ),
        (93, 1, 92, 0)
    );
    assert_eq!(atlas.census.refusals.len(), 33);
    let reason = atlas
        .census
        .refusals
        .iter()
        .find(|(_, n)| **n == 60)
        .unwrap()
        .0
        .clone();
    let mut seen = std::collections::BTreeSet::new();
    for (page, count) in [(0, 50), (1, 10), (2, 0)] {
        let text = wire::encode(&ToServer {
            id: 42,
            request: Request::AtlasRefusals {
                reason: reason.clone(),
                page,
            },
        });
        let (answer, _) = answer_text(&mut server, &text, None);
        let FromServer::Reply {
            reply: Reply::AtlasRefusals(found),
            ..
        } = wire::decode(&answer).unwrap()
        else {
            panic!("refusal cells reply")
        };
        assert_eq!(found.total, 60);
        assert_eq!(found.cells.len(), count);
        for (key, label) in found.cells {
            assert!(label.starts_with("Sheet1!A"));
            assert!(seen.insert(key));
        }
    }
    assert_eq!(seen.len(), 60);
    server
        .execute_current(Request::Intent(Intent::SetCalcMode { manual: true }), None)
        .reply
        .unwrap();
    server
        .execute_current(
            Request::Intent(Intent::EnterCell {
                at: Key::of(CellKey::new(sheet, 93, 1).unwrap()),
                text: "=3+4".into(),
            }),
            None,
        )
        .reply
        .unwrap();
    let Reply::Atlas(stale) = server.execute_current(Request::Atlas, None).reply.unwrap() else {
        panic!("atlas reply")
    };
    assert_eq!(
        (
            stale.census.stale(),
            stale.census.calculated(),
            stale.census.refused()
        ),
        (1, 0, 92)
    );
}

#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn dc_971p_12__ledger_reasons_population_and_active_pages_use_the_existing_projection() {
    use dna_calc_worker::ledger::view_model;
    let mut book = Workbook::new_blank();
    let sh = book.sheets()[0].id;
    let cell = |r| CellKey::new(sh, r, 1).unwrap();
    for row in 1..=205 {
        let formula = match row {
            1 => "=1/0".into(),
            2 => "=NoSheet!A1".into(),
            _ => format!("={row}"),
        };
        assert!(book.enter(cell(row), &formula).accepted());
    }
    let first = view_model(&book, None, "", "", "", 0, Some(cell(205)));
    assert_eq!(
        (
            first.whole,
            first.keys,
            first.rows,
            first.page.len(),
            first.active_page
        ),
        (205, 205, 205, 100, Some(2))
    );
    assert!(first.page[0].error);
    assert_eq!(first.page[0].provenance, Provenance::Calculated);
    assert!(first.page[0].reason.contains("division by zero"));
    assert_eq!(first.page[1].provenance, Provenance::Refused);
    assert!(first.page[1].reason.contains("NoSheet"));
    let last = view_model(&book, None, "", "", "", usize::MAX, Some(cell(205)));
    assert_eq!((last.page_index, last.page.len()), (2, 5));
    assert_eq!(last.page.last().unwrap().at.cell(), Some(cell(205)));
    let refused = view_model(&book, None, "refused", "", "", 0, Some(cell(205)));
    assert_eq!(
        (refused.rows, refused.whole, refused.active_page),
        (1, 205, None)
    );
    assert!(
        book.dispatch(SkinIntent::SetCalcMode(calc_host::CalcMode::Manual))
            .accepted()
    );
    assert!(book.enter(cell(3), "=300").accepted());
    let stale = view_model(&book, None, "stale", "", "", 0, Some(cell(3)));
    assert_eq!(stale.rows, 1);
    assert_eq!(stale.page[0].value, "3");
    assert!(stale.page[0].reason.contains("awaiting recalculation"));
    let imported = Workbook::open("d1.saved.xlsx", fixture()).unwrap();
    let scoped = view_model(
        &imported,
        Some(sheet(&imported, "Calc")),
        "",
        "",
        "",
        0,
        None,
    );
    assert_eq!((scoped.whole, scoped.keys), (19, 18));
}

#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn dc_971p_13__canvas_projects_cross_sheet_ranges_cycles_omissions_and_blocked_spills() {
    use dna_calc_worker::canvas::{PER_COLUMN, view_model};
    let imported = Workbook::open(
        "cut-base.xlsx",
        include_bytes!("../../../crates/calc-host/tests/fixtures/dc_gf0/cut-base.xlsx").to_vec(),
    )
    .unwrap();
    let sh = sheet(&imported, "Data");
    let at = |row, column| CellKey::new(sh, row, column).unwrap();
    let actual = view_model(&imported, at(1, 2), 1, 1);
    assert!(
        actual
            .chips
            .iter()
            .any(|c| c.reference == "Other!B3" && c.column == 1)
    );
    let ranges = view_model(&imported, at(2, 2), 1, 1);
    assert!(
        ranges
            .chips
            .iter()
            .any(|c| c.reference == "Data!A1:A3" && c.cell.is_none() && c.count == 3)
    );
    let mut book = Workbook::new_blank();
    let sh = book.sheets()[0].id;
    let at = |row, column| CellKey::new(sh, row, column).unwrap();
    assert!(book.enter(at(1, 1), "1").accepted());
    for col in 2..=21 {
        assert!(book.enter(at(1, col), "=A1").accepted());
    }
    let limited = view_model(&book, at(1, 1), 1, 1);
    assert_eq!(
        limited.chips.iter().filter(|c| c.column == 1).count(),
        PER_COLUMN
    );
    assert_eq!(limited.hidden.iter().find(|c| c.0 == 1).unwrap().3, 8);
    let mut book = Workbook::new_blank();
    let sh = book.sheets()[0].id;
    let at = |row, column| CellKey::new(sh, row, column).unwrap();
    assert!(book.enter(at(2, 4), "99").accepted());
    assert!(book.enter(at(1, 3), "=SEQUENCE(3,2)").accepted());
    let blocked = view_model(&book, at(1, 3), 1, 1);
    assert_eq!(blocked.inspector.value, "#SPILL!");
    assert_eq!(blocked.inspector.input, "=_xlfn.SEQUENCE(3,2)");
    assert!(blocked.inspector.why.contains("cannot spill"));
    // A cycle is followed once per node rather than recursively expanding forever.
    assert!(book.enter(at(10, 1), "=B10").accepted());
    assert!(book.enter(at(10, 2), "=A10").accepted());
    let cycle = view_model(&book, at(10, 1), 6, 6);
    assert_eq!(cycle.chips.len(), 2);
    assert!(cycle.width.is_finite() && cycle.height.is_finite());
}

#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn dc_971p_26__command_identity_revision_and_target_are_checked_before_effects() {
    use dna_calc_worker::{
        server::{Server, answer_text},
        wire::{self, FromServer, Intent, Key, Request, ToServer},
    };
    fn deliver(server: &mut Server, request: &Request) -> FromServer {
        // A new request id is permitted to carry a retry of the same command identity.
        let (text, _) = answer_text(
            server,
            &wire::encode(&ToServer {
                id: 99,
                request: request.clone(),
            }),
            None,
        );
        wire::decode(&text).unwrap()
    }
    fn accepted(reply: FromServer) {
        let FromServer::Reply {
            reply: wire::Reply::Done { outcome, .. },
            ..
        } = reply
        else {
            panic!("{reply:?}")
        };
        assert!(outcome.accepted, "{outcome:?}");
    }
    fn refused(reply: FromServer, reason: &str) {
        let FromServer::Failed { message, .. } = reply else {
            panic!("{reply:?}")
        };
        assert!(message.contains(reason), "{message}");
    }
    let mut server = Server::new();
    let sheet = server.book().sheets()[0].id;
    let at = CellKey::new(sheet, 1, 1).unwrap();
    let stale = server.envelope(Request::Intent(Intent::EnterCell {
        at: Key::of(at),
        text: "stale".into(),
    }));
    let edit = server.envelope(Request::Intent(Intent::EnterCell {
        at: Key::of(at),
        text: "42".into(),
    }));
    let mut other = Server::new();
    refused(deliver(&mut other, &edit), "another workbook session");
    accepted(deliver(&mut server, &edit));
    server.take_events();
    let revision = server.book().host().revision();
    let depth = server.book().session().history_depth();
    let first = deliver(&mut server, &edit);
    accepted(first);
    assert_eq!(server.book().host().revision(), revision);
    assert_eq!(server.book().session().history_depth(), depth);
    assert!(server.take_events().is_empty());
    // The same identity with a different payload cannot be reinterpreted as a new command.
    refused(deliver(&mut server, &stale), "already used");
    let mut stale = server.envelope(Request::Intent(Intent::EnterCell {
        at: Key::of(at),
        text: "stale".into(),
    }));
    if let Request::Command(ref mut e) = stale {
        e.base_revision = revision.get() - 1;
    }
    refused(deliver(&mut server, &stale), "revision");
    assert_eq!(shown(server.book(), at).text, "42");
    assert_eq!(server.book().session().history_depth(), depth);
    let undo = server.envelope(Request::Intent(Intent::Undo));
    accepted(deliver(&mut server, &undo));
    let undone = server.book().host().revision();
    accepted(deliver(&mut server, &undo));
    assert_eq!(server.book().host().revision(), undone);
    assert_eq!(shown(server.book(), at).text, "");
    let mut moved = server.envelope(Request::Intent(Intent::EnterSelection {
        text: "wrong target".into(),
    }));
    if let Request::Command(ref mut e) = moved {
        e.resolved_selection = Some((Key::of(at), vec![wire::Area::of(GridArea::cell(at))]));
    }
    refused(deliver(&mut server, &moved), "resolved selection");
    let old = server.envelope(Request::Intent(Intent::Recalculate));
    let new = server.envelope(Request::New);
    accepted(deliver(&mut server, &new));
    let workbook = if let FromServer::Reply {
        reply: wire::Reply::Done { outcome, .. },
        ..
    } = deliver(&mut server, &new)
    {
        outcome.summary.workbook
    } else {
        panic!()
    };
    let mut old = old;
    if let Request::Command(ref mut e) = old {
        e.id += 1;
    }
    refused(deliver(&mut server, &old), "replaced");
    let now = server.envelope(Request::Intent(Intent::Recalculate));
    accepted(deliver(&mut server, &now));
    assert!(
        matches!(server.execute_current(Request::Save,None).reply,Ok(wire::Reply::Done {outcome,..}) if outcome.summary.workbook==workbook)
    );
    // Bare transport commands cannot bypass admission.
    refused(
        deliver(&mut server, &Request::Intent(Intent::Undo)),
        "identity",
    );
}

#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn dc_971p_26__evicted_command_and_calculation_retries_never_run_again() {
    use dna_calc_worker::{
        server::{Server, answer_text},
        wire::{self, FromServer, Intent, Key, Request, ToServer},
    };
    let mut server = Server::new();
    let sheet = server.book().sheets()[0].id;
    let at = CellKey::new(sheet, 1, 1).unwrap();
    let command = server.envelope(Request::Intent(Intent::EnterCell {
        at: Key::of(at),
        text: "=1+2".into(),
    }));
    let send = |server: &mut Server, request: &Request| {
        let (text, _) = answer_text(
            server,
            &wire::encode(&ToServer {
                id: 1,
                request: request.clone(),
            }),
            None,
        );
        wire::decode::<FromServer>(&text).unwrap()
    };
    assert!(matches!(
        send(&mut server, &command),
        FromServer::Reply { .. }
    ));
    let calc = server.envelope(Request::Intent(Intent::Recalculate));
    assert!(matches!(send(&mut server, &calc), FromServer::Reply { .. }));
    server.take_events();
    assert!(matches!(send(&mut server, &calc), FromServer::Reply { .. }));
    assert!(
        server.take_events().is_empty(),
        "retry published another calculation"
    );
    let revision = server.book().host().revision();
    let depth = server.book().session().history_depth();
    for _ in 0..130 {
        let select = server.envelope(Request::Intent(Intent::Select {
            active: Key::of(at),
            areas: vec![wire::Area::of(GridArea::cell(at))],
        }));
        assert!(matches!(
            send(&mut server, &select),
            FromServer::Reply { .. }
        ));
    }
    assert!(
        matches!(send(&mut server,&command),FromServer::Failed {message,..} if message.contains("expired"))
    );
    assert_eq!(server.book().host().revision(), revision);
    assert_eq!(server.book().session().history_depth(), depth);
    assert_eq!(shown(server.book(), at).text, "3");
}
