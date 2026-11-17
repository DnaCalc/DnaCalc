//! The envelope's rules (docs/architecture/ADDIN_SURROGATE.md sections 4, 6 and 8), driven through
//! the two ends of a link with the bytes one writes handed to the other, as a pipe would.

use calc_surrogate_wire::{
    ControllerLink, Decoder, Flow, Frame, HEADER_LEN, Header, Incarnation, Kind, Outbound,
    Refusal, SurrogateIncoming, SurrogateLink, Incoming, UNKNOWN_OUTCOME, WireError, encode,
};

const CALL: Kind = Kind(1);
const ANSWER: Kind = Kind(2);
const EVENT: Kind = Kind(3);
const ASK: Kind = Kind(4);
const TOLD: Kind = Kind(5);
const LIMIT: usize = 1 << 20;

fn incarnation(seed: u8, start: u32) -> Incarnation {
    Incarnation::new([seed; 16], start)
}

fn pair(i: Incarnation) -> (ControllerLink, SurrogateLink) {
    (ControllerLink::new(i, LIMIT), SurrogateLink::new(i, LIMIT))
}

/// Everything the controller has from the surrogate, until it has nothing more.
fn drain(controller: &mut ControllerLink) -> Vec<Incoming> {
    let mut out = Vec::new();
    while let Some(next) = controller.next().expect("no fatal error") {
        out.push(next);
    }
    out
}

fn drain_surrogate(surrogate: &mut SurrogateLink) -> Vec<SurrogateIncoming> {
    let mut out = Vec::new();
    while let Some(next) = surrogate.next().expect("no fatal error") {
        out.push(next);
    }
    out
}

/// Starts a link: the surrogate's hello reaches the controller.
fn started(i: Incarnation) -> (ControllerLink, SurrogateLink) {
    let (mut controller, mut surrogate) = pair(i);
    controller.receive(&surrogate.hello());
    assert_eq!(drain(&mut controller), vec![Incoming::Hello]);
    assert!(controller.ready());
    (controller, surrogate)
}

#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn a_frame_round_trips_with_every_envelope_field() {
    let header = Header {
        incarnation: Incarnation::new(*b"0123456789abcdef", 7),
        flow: Flow::Request,
        kind: Kind(0x1234),
        sequence: 9,
        request: 41,
        parent: 40,
        length: 3,
    };
    let bytes = encode(&header, b"abc");
    assert_eq!(bytes.len(), HEADER_LEN + 3);
    let mut decoder = Decoder::new(LIMIT);
    // A frame split anywhere is whole only once its last byte arrives.
    decoder.push(&bytes[..HEADER_LEN - 1]);
    assert_eq!(decoder.next_frame(), Ok(None));
    decoder.push(&bytes[HEADER_LEN - 1..]);
    assert_eq!(
        decoder.next_frame(),
        Ok(Some(Frame {
            header,
            payload: b"abc".to_vec()
        }))
    );
    assert_eq!(decoder.next_frame(), Ok(None));
}

#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn the_header_and_size_are_checked_before_the_payload_is_read() {
    let mut out = Outbound::new(incarnation(1, 1));
    let good = out.frame(Flow::Event, EVENT, 0, 0, b"x");
    // A wrong magic or version ends the connection at once.
    let mut bad = good.clone();
    bad[0] = b'X';
    let mut decoder = Decoder::new(LIMIT);
    decoder.push(&bad);
    assert_eq!(decoder.next_frame(), Err(WireError::Magic));
    let mut bad = good.clone();
    bad[4] = 99;
    let mut decoder = Decoder::new(LIMIT);
    decoder.push(&bad);
    assert!(matches!(decoder.next_frame(), Err(WireError::Version(_))));
    // A flow the envelope does not define.
    let mut bad = good.clone();
    bad[6] = 9;
    let mut decoder = Decoder::new(LIMIT);
    decoder.push(&bad);
    assert_eq!(decoder.next_frame(), Err(WireError::Flow(9)));
    // A length over the limit fails on the header alone: no payload byte has arrived.
    let header = Header {
        incarnation: incarnation(1, 1),
        flow: Flow::Event,
        kind: EVENT,
        sequence: 1,
        request: 0,
        parent: 0,
        length: (LIMIT + 1) as u32,
    };
    let big = encode(&header, &[]);
    let mut decoder = Decoder::new(LIMIT);
    decoder.push(&big[..HEADER_LEN]);
    assert_eq!(
        decoder.next_frame(),
        Err(WireError::TooLarge {
            length: LIMIT + 1,
            limit: LIMIT
        })
    );
    // After an error the decoder stays failed.
    decoder.push(&good);
    assert!(decoder.next_frame().is_err());
}

#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn a_request_is_answered_by_the_reply_that_names_it() {
    let (mut controller, mut surrogate) = started(incarnation(2, 1));
    let (request, bytes) = controller.request(CALL, ANSWER, b"=1+1".to_vec()).unwrap();
    surrogate.receive(&bytes);
    assert_eq!(
        drain_surrogate(&mut surrogate),
        vec![SurrogateIncoming::Request {
            request,
            kind: CALL,
            payload: b"=1+1".to_vec()
        }]
    );
    controller.receive(&surrogate.reply(request, ANSWER, b"2".to_vec()).unwrap());
    assert_eq!(
        drain(&mut controller),
        vec![Incoming::Reply {
            request,
            kind: ANSWER,
            payload: b"2".to_vec()
        }]
    );
    assert!(controller.open().is_empty());
    // The surrogate answers only a request it holds, once.
    assert!(surrogate.reply(request, ANSWER, Vec::new()).is_err());
}

#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn a_reply_of_another_kind_ends_the_connection_before_its_payload_is_used() {
    let (mut controller, mut surrogate) = started(incarnation(3, 1));
    let (request, bytes) = controller.request(CALL, ANSWER, Vec::new()).unwrap();
    surrogate.receive(&bytes);
    drain_surrogate(&mut surrogate);
    controller.receive(&surrogate.reply(request, EVENT, b"?".to_vec()).unwrap());
    assert_eq!(
        controller.next(),
        Err(WireError::Kind {
            request,
            expected: ANSWER,
            got: EVENT
        })
    );
    assert!(!controller.ready());
}

#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn a_gap_or_a_repeat_in_the_sequence_ends_the_connection() {
    let i = incarnation(4, 1);
    // A repeat: the surrogate's hello arrives twice.
    let (mut controller, mut surrogate) = pair(i);
    let hello = surrogate.hello();
    controller.receive(&hello);
    controller.receive(&hello);
    assert_eq!(controller.next(), Ok(Some(Incoming::Hello)));
    assert_eq!(
        controller.next(),
        Err(WireError::Sequence {
            expected: 2,
            got: 1
        })
    );
    // A gap: one event is lost on the way.
    let (mut controller, mut surrogate) = started(i);
    let _lost = surrogate.event(EVENT, b"rtd".to_vec());
    controller.receive(&surrogate.event(EVENT, b"rtd".to_vec()));
    assert_eq!(
        controller.next(),
        Err(WireError::Sequence {
            expected: 2,
            got: 3
        })
    );
    // Once failed, a link takes nothing more.
    assert!(controller.request(CALL, ANSWER, Vec::new()).is_err());
}

#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn nothing_but_the_hello_is_taken_before_it_and_a_request_waits_for_it() {
    let i = incarnation(5, 1);
    let (mut controller, mut surrogate) = pair(i);
    assert!(!controller.ready());
    assert!(controller.request(CALL, ANSWER, Vec::new()).is_err());
    // An event before the hello is a broken surrogate.
    controller.receive(&surrogate.event(EVENT, Vec::new()));
    assert_eq!(controller.next(), Err(WireError::BeforeHello));
    assert!(!controller.ready());
}

#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn a_callback_names_the_open_request_and_is_served_while_it_waits() {
    let (mut controller, mut surrogate) = started(incarnation(6, 1));
    let (request, bytes) = controller.request(CALL, ANSWER, b"open".to_vec()).unwrap();
    // With no controller request open, the surrogate has nothing to ask under.
    assert!(surrogate.callback(ASK, TOLD, b"pick".to_vec()).is_err());
    surrogate.receive(&bytes);
    drain_surrogate(&mut surrogate);
    let (callback, ask) = surrogate.callback(ASK, TOLD, b"pick".to_vec()).unwrap();
    controller.receive(&ask);
    assert_eq!(
        drain(&mut controller),
        vec![Incoming::Callback {
            request: callback,
            parent: request,
            kind: ASK,
            payload: b"pick".to_vec()
        }]
    );
    surrogate.receive(&controller.answer_callback(callback, TOLD, b"a.xlsx".to_vec()).unwrap());
    assert_eq!(
        drain_surrogate(&mut surrogate),
        vec![SurrogateIncoming::CallbackReply {
            request: callback,
            kind: TOLD,
            payload: b"a.xlsx".to_vec()
        }]
    );
    // The controller answers a callback once.
    assert!(controller.answer_callback(callback, TOLD, Vec::new()).is_err());
    controller.receive(&surrogate.reply(request, ANSWER, b"opened".to_vec()).unwrap());
    assert_eq!(drain(&mut controller).len(), 1);
}

#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn a_callback_under_a_request_the_controller_stopped_waiting_for_is_refused() {
    let (mut controller, mut surrogate) = started(incarnation(7, 1));
    let (request, bytes) = controller.request(CALL, ANSWER, Vec::new()).unwrap();
    surrogate.receive(&bytes);
    drain_surrogate(&mut surrogate);
    // The call took longer than its limit: the controller stops waiting (it does not stop the code).
    controller.cancel(request);
    let (callback, ask) = surrogate.callback(ASK, TOLD, Vec::new()).unwrap();
    controller.receive(&ask);
    assert_eq!(
        drain(&mut controller),
        vec![Incoming::Refused(Refusal::Parent {
            request: callback,
            parent: request
        })]
    );
    // The surrogate's code is told so, instead of waiting for an answer that never comes.
    surrogate.receive(&controller.decline_callback(callback).unwrap());
    assert_eq!(
        drain_surrogate(&mut surrogate),
        vec![SurrogateIncoming::Declined { request: callback }]
    );
    assert!(controller.decline_callback(callback).is_err());
    // Its late result is not admitted, and the link stays up.
    controller.receive(&surrogate.reply(request, ANSWER, b"late".to_vec()).unwrap());
    assert_eq!(
        drain(&mut controller),
        vec![Incoming::Refused(Refusal::Late { request })]
    );
    assert!(controller.ready());
    let (next, bytes) = controller.request(CALL, ANSWER, Vec::new()).unwrap();
    assert!(next > request);
    surrogate.receive(&bytes);
    assert_eq!(drain_surrogate(&mut surrogate).len(), 1);
}

#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn a_reply_to_a_request_never_made_is_refused() {
    let i = incarnation(8, 1);
    let (mut controller, _surrogate) = started(i);
    let mut forged = Outbound::new(i);
    // The forged frames continue the surrogate's sequence (its hello was 1).
    forged.skip_to(2);
    controller.receive(&forged.frame(Flow::Reply, ANSWER, 77, 0, b"x"));
    assert_eq!(
        drain(&mut controller),
        vec![Incoming::Refused(Refusal::NotOpen { request: 77 })]
    );
}

#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn after_a_restart_no_message_from_the_old_incarnation_is_accepted() {
    let old = incarnation(9, 1);
    let (mut controller, mut surrogate) = started(old);
    let (request, bytes) = controller.request(CALL, ANSWER, b"slow".to_vec()).unwrap();
    surrogate.receive(&bytes);
    drain_surrogate(&mut surrogate);
    // The surrogate stops: every open call gets the unknown outcome, once.
    let stopped = controller.stopped();
    assert_eq!(stopped, vec![request]);
    assert!(!controller.ready());
    assert_eq!(
        UNKNOWN_OUTCOME,
        "The add-in process stopped before this result was accepted. External work may have completed. Retrying will run the add-in again."
    );
    // Its late answer, an RTD signal and an async completion, all of the old incarnation, reach
    // the replacement's link: none is accepted, and none counts in the new sequence.
    let late = surrogate.reply(request, ANSWER, b"done".to_vec()).unwrap();
    let rtd = surrogate.event(EVENT, b"rtd".to_vec());
    let completion = surrogate.event(EVENT, b"async 1".to_vec());
    let new = Incarnation::new([9; 16], 2);
    assert_ne!(old, new);
    let (mut replacement, mut fresh) = pair(new);
    for bytes in [late, rtd, completion] {
        replacement.receive(&bytes);
    }
    replacement.receive(&fresh.hello());
    let got = drain(&mut replacement);
    assert_eq!(
        got,
        vec![
            Incoming::Refused(Refusal::Incarnation { got: old }),
            Incoming::Refused(Refusal::Incarnation { got: old }),
            Incoming::Refused(Refusal::Incarnation { got: old }),
            Incoming::Hello,
        ]
    );
    // Nothing re-runs by itself: the replacement holds no open request.
    assert!(replacement.open().is_empty());
    // And the replacement's surrogate refuses frames meant for the old one.
    let mut stale = Outbound::new(old);
    fresh.receive(&stale.frame(Flow::Request, CALL, 1, 0, b""));
    assert_eq!(
        drain_surrogate(&mut fresh),
        vec![SurrogateIncoming::Refused(Refusal::Incarnation { got: old })]
    );
}

#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn incarnations_with_the_same_random_bits_differ_by_their_start_count() {
    assert_ne!(incarnation(1, 1), incarnation(1, 2));
    assert_ne!(incarnation(1, 1), incarnation(2, 1));
    let i = Incarnation::from_parts(0x0102_0304_0506_0708_090a_0b0c_0d0e_0f10, 3);
    assert_eq!(i.nonce[0], 0x01);
    assert_eq!(i.nonce[15], 0x10);
    assert_eq!(i.start, 3);
}

#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn reserved_kinds_are_not_the_applications() {
    let (mut controller, _surrogate) = started(incarnation(10, 1));
    assert!(controller.request(Kind::HELLO, ANSWER, Vec::new()).is_err());
    assert!(controller.request(CALL, Kind::HELLO, Vec::new()).is_err());
}
