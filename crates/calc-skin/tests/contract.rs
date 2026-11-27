use calc_skin::{
    AnchorId, Carrier, Envelope, Frame, InProcess, Message, Packet, Refusal, Revision, Stamp,
    WorkbookId,
};
use std::num::NonZeroU128;

#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn a_frame_and_its_binary_payload_cross_the_same_carrier() {
    let (page, host) = InProcess::pair(2);
    let packet = Packet {
        envelope: Envelope::new(
            3,
            Message::Frame(Frame {
                stamp: Stamp {
                    workbook: WorkbookId(2),
                    model: Revision(7),
                    view: Revision(11),
                    calc: 13.into(),
                },
                events: Vec::new(),
                updates: Vec::new(),
            }),
        ),
        bytes: Some(vec![0, 255, 1]),
    };
    host.send(&packet).unwrap();
    assert_eq!(page.receive().unwrap(), Some(packet));
    assert_eq!(page.receive().unwrap(), None);

    let mut foreign = Envelope::new(4, Message::Failure(Refusal::ReadOnly));
    foreign.schema += 1;
    host.send(&Packet {
        envelope: foreign,
        bytes: None,
    })
    .unwrap();
    assert!(matches!(
        page.receive(),
        Err(Refusal::SchemaMismatch { .. })
    ));
}

#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn stable_ids_round_trip_as_strings_without_javascript_number_loss() {
    let id = AnchorId(NonZeroU128::MAX);
    let json = serde_json::to_string(&id).unwrap();
    assert_eq!(json, "\"ffffffff-ffff-ffff-ffff-ffffffffffff\"");
    assert_eq!(serde_json::from_str::<AnchorId>(&json).unwrap(), id);
    assert!(serde_json::from_str::<AnchorId>("\"00000000-0000-0000-0000-000000000000\"").is_err());
}
