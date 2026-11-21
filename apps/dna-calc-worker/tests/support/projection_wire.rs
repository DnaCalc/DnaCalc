//! Exercise the same text protocol as the browser worker on saved workbook fixtures.

use dna_calc_worker::{
    server::{Server, answer_text},
    wire::{self, Area, FromServer, Key, Reply, Request, Sheet, ToServer},
};

fn send(server: &mut Server, id: u64, request: Request, bytes: Option<Vec<u8>>) -> (String, Reply) {
    let (text, out) = answer_text(server, &wire::encode(&ToServer { id, request }), bytes);
    assert!(out.is_none(), "projection {id} also sent bytes");
    let FromServer::Reply {
        id: answered,
        reply,
    } = wire::decode(&text).unwrap()
    else {
        panic!("projection {id} failed: {text}");
    };
    assert_eq!(id, answered);
    (text, reply)
}

pub(super) fn projections() -> Vec<u8> {
    let workbooks: [(&str, &[u8]); 2] = [
        (
            "notebook.xlsx",
            include_bytes!("../../../../crates/calc-doc/tests/fixtures/cdoc_m2_1/notebook.xlsx"),
        ),
        (
            "chart-kinds.xlsx",
            include_bytes!("../../../../crates/calc-doc/tests/fixtures/drawing/chart-kinds.xlsx"),
        ),
    ];
    let mut output = Vec::new();
    let mut charts = 0;
    for (name, bytes) in workbooks {
        let mut server = Server::new();
        let open = server.envelope(Request::Open { name: name.into() });
        let (_, opened) = send(&mut server, 1, open, Some(bytes.to_vec()));
        assert!(matches!(opened, Reply::Done { ref outcome, .. } if outcome.accepted));
        while server.has_pending_calculation() {
            server.advance_calculation();
            server.take_events();
        }
        let sheet_id = server.book().sheets()[0].id;
        let sheet = Sheet::of(sheet_id);
        let center = server
            .book()
            .host()
            .formula_cells(sheet_id)
            .first()
            .copied()
            .map_or(Key(sheet.0, sheet.1, 1, 1), Key::of);
        for (index, request) in [
            Request::Outline {
                shown: vec![(sheet, 0)],
                open: vec![sheet],
                reveal: Some(center),
                query: String::new(),
            },
            Request::Notebook {
                sheet,
                page: 0,
                reveal: Some(center),
                placement: None,
            },
            Request::BlockDetail(Area(center, center)),
            Request::Canvas {
                center,
                back: 2,
                forward: 2,
            },
            Request::Ledger {
                sheet: Some(sheet),
                filter: "all".into(),
                text: String::new(),
                sort: "address".into(),
                page: 0,
                active: Some(center),
            },
            Request::Charts(sheet),
        ]
        .into_iter()
        .enumerate()
        {
            let (text, reply) = send(&mut server, index as u64 + 2, request, None);
            if let Reply::Charts(rows) = reply {
                charts += rows.len();
            }
            output.extend_from_slice(text.as_bytes());
            output.push(b'\n');
        }
    }
    assert!(charts > 0, "fixtures must exercise chart serialization");
    output
}

#[cfg(test)]
#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn skin_projections_keep_their_wire_bytes() {
    assert_eq!(
        projections(),
        include_bytes!("../fixtures/skin-projections.jsonl").as_slice()
    );
}
