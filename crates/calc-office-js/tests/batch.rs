use calc_func::UserFunctionContext;
use calc_host::automation::{AutomationHost, WorkbookAutomation};
use calc_host::{HostOptions, HostSession, MemorySource, OpenPolicy, blank_package};
use calc_object_model::profile::ApplicationHost;
use calc_object_model::{AutoError, AutoValue, Call, ObjectKey, ObjectModel, ObjectRef};
use calc_office_js::{BatchReply, BatchRequest, Executor, Op, WireValue};
use dna_calc_protocol::{
    FromServer, Reply, Request, ToServer, automation::ReleaseRequest, decode, encode,
};
use std::cell::{Cell, RefCell};
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

fn batch<M: ObjectModel>(host: &Executor<M>, id: u64, ops: Vec<Op>) -> BatchReply {
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

fn request_through_wire<M: ObjectModel>(host: &Executor<M>, id: u64, request: Request) -> Reply {
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

#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn loading_nullness_with_properties_skips_only_missing_objects() {
    let host = host();
    let mut ops = range();
    for (out, key, props) in [
        (5, "Missing", ["isNullObject", "name"]),
        (6, "Absent", ["name", "isNullObject"]),
        (7, "Sheet1", ["name", "isNullObject"]),
    ] {
        ops.extend([
            Op::Call {
                target: 2,
                name: "getItemOrNullObject".into(),
                args: vec![WireValue::text(key)],
                out: Some(out),
            },
            Op::Load {
                target: out,
                props: props.into_iter().map(String::from).collect(),
            },
        ]);
    }

    let reply = batch(&host, 1, ops);
    assert_eq!(reply.error, None);
    for ordinal in [4, 6] {
        assert!(reply.results[ordinal].null_object);
        assert_eq!(
            reply.results[ordinal + 1].loaded,
            [("isNullObject".into(), WireValue::Bool { v: true })]
        );
    }
    assert!(!reply.results[8].null_object);
    assert_eq!(
        reply.results[9].loaded,
        [
            ("name".into(), WireValue::text("Sheet1")),
            ("isNullObject".into(), WireValue::Bool { v: false }),
        ]
    );
}

#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn workbook_roots_balance_temporary_application_acquisitions() {
    struct CountedModel<'a> {
        inner: &'a WorkbookAutomation,
        held: Cell<usize>,
        fail_active_workbook: Cell<bool>,
    }

    impl ObjectModel for CountedModel<'_> {
        fn application(&self) -> ObjectRef {
            self.held.set(self.held.get() + 1);
            self.inner.application()
        }

        fn invoke(&self, target: &ObjectRef, call: &Call<'_>) -> Outcome {
            if call.member.name == "ActiveWorkbook" && self.fail_active_workbook.get() {
                return Err(AutoError::Status(0x8002_000Bu32 as i32));
            }
            self.inner.invoke(target, call)
        }

        fn released(&self, target: &ObjectRef) {
            if target.key == ObjectKey::Application {
                self.held.set(self.held.get() - 1);
            }
            self.inner.released(target);
        }
    }

    let real = host();
    let host = Executor::new(CountedModel {
        inner: real.model(),
        held: Cell::new(0),
        fail_active_workbook: Cell::new(false),
    });
    let root = |id, name: &str| {
        batch(
            &host,
            id,
            vec![Op::Root {
                name: name.into(),
                out: id,
            }],
        )
    };

    assert_eq!(root(1, "workbook").error, None);
    assert_eq!(host.model().held.get(), 0, "successful workbook lookup");
    host.model().fail_active_workbook.set(true);
    assert_eq!(root(2, "workbook").error.unwrap().code, "ItemNotFound");
    assert_eq!(host.model().held.get(), 0, "failed workbook lookup");
    assert_eq!(root(3, "unknown").error.unwrap().code, "ApiNotFound");
    assert_eq!(host.model().held.get(), 0, "unknown root");
    assert_eq!(root(4, "application").error, None);
    assert_eq!(host.model().held.get(), 1, "bound application root");
    host.release(1);
    assert_eq!(host.model().held.get(), 0, "released application root");
}
