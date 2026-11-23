use calc_func::UserFunctionContext;
use calc_host::automation::{AutomationHost, WorkbookAutomation};
use calc_host::{HostOptions, HostSession, MemorySource, OpenPolicy, blank_package};
use calc_object_model::profile::ApplicationHost;
use calc_object_model::{AutoError, AutoValue};
use calc_office_js::{BatchReply, BatchRequest, Executor, Op, WireValue};
use dna_calc_protocol::{
    FromServer, Reply, Request, ToServer, automation::ReleaseRequest, decode, encode,
};
use std::cell::RefCell;
use std::rc::Rc;
use std::sync::Arc;

type Outcome = Result<AutoValue, AutoError>;

struct Headless(RefCell<HostSession>);

impl ApplicationHost for Headless {
    fn hwnd(&self) -> isize {
        0
    }
    fn run(&self, _: &str, _: &[AutoValue]) -> Outcome {
        Err(AutoError::not_modelled("macro invocation", "dc-17spk.5"))
    }
    fn events_enabled(&self) -> bool {
        self.0.borrow().workbook_events_switch().get()
    }
    fn set_events_enabled(&self, enabled: bool) {
        self.0.borrow().workbook_events_switch().set(enabled);
    }
}

impl AutomationHost for Headless {
    fn with_invocation(
        &self,
        _: &mut dyn FnMut(&mut dyn UserFunctionContext) -> Outcome,
    ) -> Option<Outcome> {
        None
    }
    fn with_session(&self, call: &mut dyn FnMut(&HostSession) -> Outcome) -> Outcome {
        call(&self.0.borrow())
    }
    fn with_session_mut(&self, call: &mut dyn FnMut(&mut HostSession) -> Outcome) -> Outcome {
        call(&mut self.0.borrow_mut())
    }
}

fn host() -> Executor<WorkbookAutomation> {
    let session = HostSession::open(
        Arc::new(MemorySource::new(blank_package())),
        &OpenPolicy::default(),
        HostOptions::default(),
    )
    .unwrap();
    let host = Rc::new(Headless(RefCell::new(session)));
    let model = WorkbookAutomation::new(host.clone());
    model.context_changed(&host.0.borrow());
    Executor::new(model)
}

fn batch(host: &Executor<WorkbookAutomation>, id: u64, ops: Vec<Op>) -> BatchReply {
    let request = BatchRequest {
        batch_id: id,
        context: 1,
        ops,
        final_release: false,
    };
    let Reply::AutomationBatch(reply) =
        request_through_wire(host, id, Request::AutomationBatch(request))
    else {
        panic!("expected a batch reply")
    };
    reply
}

fn request_through_wire(host: &Executor<WorkbookAutomation>, id: u64, request: Request) -> Reply {
    let message = decode::<ToServer>(&encode(&ToServer { id, request })).unwrap();
    let reply = host.handle(&message.request).unwrap();
    let response = encode(&FromServer::Reply {
        id: message.id,
        reply,
    });
    let FromServer::Reply {
        id: response_id,
        reply,
    } = decode(&response).unwrap()
    else {
        panic!("expected a reply envelope")
    };
    assert_eq!(response_id, id);
    reply
}

fn range() -> Vec<Op> {
    vec![
        Op::Root {
            name: "workbook".into(),
            out: 1,
        },
        Op::Get {
            target: 1,
            name: "worksheets".into(),
            out: 2,
        },
        Op::Call {
            target: 2,
            name: "getItem".into(),
            args: vec![WireValue::text("Sheet1")],
            out: Some(3),
        },
        Op::Call {
            target: 3,
            name: "getRange".into(),
            args: vec![WireValue::text("A1")],
            out: Some(4),
        },
    ]
}

fn load(prop: &str) -> Op {
    Op::Load {
        target: 4,
        props: vec![prop.into()],
    }
}

fn put(value: f64) -> Op {
    Op::Set {
        target: 4,
        name: "values".into(),
        value: WireValue::Array {
            rows: 1,
            cols: 1,
            v: vec![WireValue::r8(value)],
        },
    }
}

#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn dc_17spk_5__ordered_snapshots_and_failure_keep_the_prefix() {
    let host = host();
    let mut ops = range();
    ops.extend([
        put(2.0),
        load("values"),
        put(3.0),
        load("values"),
        load("unknown"),
        put(99.0),
    ]);
    let reply = batch(&host, 1, ops);
    let failure = reply.error.unwrap();
    assert_eq!(
        (
            failure.ordinal,
            failure.code.as_str(),
            failure.owner.as_deref()
        ),
        (8, "ApiNotFound", Some("dc-17spk.5"))
    );
    assert_eq!(reply.results.len(), 8);
    assert_eq!(reply.results[5].loaded[0].1, WireValue::r8(2.0));
    assert_eq!(reply.results[7].loaded[0].1, WireValue::r8(3.0));
    assert!(batch(&host, 1, vec![put(99.0)]).error.is_some());
    let next = batch(
        &host,
        2,
        vec![load("values"), load("address"), load("rowIndex")],
    );
    assert_eq!(next.error, None);
    assert_eq!(next.results[0].loaded[0].1, WireValue::r8(3.0));
    assert_eq!(next.results[1].loaded[0].1, WireValue::text("Sheet1!A1"));
    assert_eq!(next.results[2].loaded[0].1, WireValue::I4 { v: 0 });
    assert_eq!(
        request_through_wire(
            &host,
            3,
            Request::AutomationRelease(ReleaseRequest { context: 1 })
        ),
        Reply::AutomationReleased
    );
    assert_eq!(host.live_handles(), 0);
    assert_eq!(
        batch(&host, 4, vec![load("values")]).error.unwrap().code,
        "InvalidObjectPath"
    );
}

#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn dc_17spk_5__only_item_not_found_becomes_a_null_object() {
    let host = host();
    let mut ops = range();
    ops.extend([
        Op::Call {
            target: 2,
            name: "getItemOrNullObject".into(),
            args: vec![WireValue::text("Missing")],
            out: Some(5),
        },
        Op::Load {
            target: 5,
            props: vec!["isNullObject".into()],
        },
        Op::Call {
            target: 2,
            name: "getItemOrNullObject".into(),
            args: vec![],
            out: Some(6),
        },
    ]);
    let reply = batch(&host, 1, ops);
    assert!(reply.results[4].null_object);
    assert_eq!(reply.results[5].loaded[0].1, WireValue::Bool { v: true });
    assert_eq!(reply.error.unwrap().ordinal, 6);
}
