//! Owning-STA RTD client. Notifications carry no evaluator or Rust pointers.
use crate::projection::Value;
use calc_value::{CalcValue, CellKey, ExcelText, WorksheetErrorCode};
use calc_xll_host::{RtdProvider, RtdService};
use std::{
    cell::RefCell,
    collections::{BTreeMap, BTreeSet},
    ffi::c_void,
};

unsafe extern "C" {
    fn dna_rtd_create() -> *mut c_void;
    fn dna_rtd_connect(
        client: *mut c_void,
        progid: *const u16,
        topic: i32,
        strings: *const *const u16,
        count: usize,
        out: *mut Value,
    ) -> i32;
    fn dna_rtd_poll(
        client: *mut c_void,
        publish: unsafe extern "system" fn(*mut c_void, i32, *const Value) -> i32,
        context: *mut c_void,
    ) -> i32;
    fn dna_rtd_disconnect(client: *mut c_void, topic: i32) -> i32;
    fn dna_rtd_close(client: *mut c_void) -> i32;
    fn dna_rtd_destroy(client: *mut c_void);
    fn dna_rtd_value_clear(value: *mut Value);
}
fn checked(code: i32, operation: &str) -> Result<(), String> {
    if code < 0 {
        Err(format!("dc-snz.23: RTD {operation}: {code:#x}"))
    } else {
        Ok(())
    }
}
fn wide(text: &str) -> Result<Vec<u16>, String> {
    if text.contains('\0') {
        return Err("dc-snz.23: RTD string contains NUL".into());
    }
    Ok(text.encode_utf16().chain(Some(0)).collect())
}
unsafe fn decode(value: &Value) -> Result<CalcValue, String> {
    Ok(match value.kind {
        // An empty value is also an RTD input. Its advancing sequence makes
        // a signaling provider's separate result cache observable to the owner.
        // The cached UDF result still has to declare this admitted input read.
        0 => CalcValue::empty(),
        1 if value.number.is_finite() => CalcValue::number(value.number),
        2 => CalcValue::text(ExcelText::from_utf16_code_units(if value.length == 0 {
            Vec::new()
        } else {
            unsafe { std::slice::from_raw_parts(value.text, value.length) }.to_vec()
        })),
        3 => CalcValue::logical(value.number != 0.),
        5 => CalcValue::error(match value.number as u32 & 0xffff {
            2000 => WorksheetErrorCode::Null,
            2007 => WorksheetErrorCode::Div0,
            2015 => WorksheetErrorCode::Value,
            2023 => WorksheetErrorCode::Ref,
            2029 => WorksheetErrorCode::Name,
            2036 => WorksheetErrorCode::Num,
            2042 => WorksheetErrorCode::NA,
            2043 => WorksheetErrorCode::GettingData,
            _ => return Err("dc-snz.23: RTD error variant is not modelled".into()),
        }),
        _ => return Err("dc-snz.23: RTD value variant is not modelled".into()),
    })
}
#[derive(Clone, Debug)]
pub struct RtdInput {
    pub provider: RtdProvider,
    pub subscription: u64,
    pub sequence: u64,
    pub value: CalcValue,
}
struct Topic {
    provider: RtdProvider,
    progid: String,
    strings: Vec<String>,
    callers: BTreeSet<CellKey>,
    // Keep the last input's logical lease until the owner retires it. Only
    // callers that still declare RTD keep its native transport connected.
    active_callers: BTreeSet<CellKey>,
    connected: bool,
    transport: i32,
    sequence: u64,
}
struct Declaration {
    provider: RtdProvider,
    caller: CellKey,
    subscriptions: BTreeSet<u64>,
}
struct PendingDisconnect {
    provider: RtdProvider,
    caller: CellKey,
    transport: i32,
}
struct State {
    pointer: *mut c_void,
    topics: BTreeMap<u64, Topic>,
    pending_disconnects: Vec<PendingDisconnect>,
    declarations: BTreeMap<u64, Declaration>,
    next: u64,
    next_transport: i32,
    transports: BTreeMap<i32, u64>,
    updates: Vec<RtdInput>,
    closed: bool,
    failure: Option<String>,
    close_failure: Option<String>,
    activated: bool,
}
/// Thread-bound by RefCell and the native owner's thread check. It never registers a COM class.
pub struct RtdClient {
    state: RefCell<State>,
}
impl RtdClient {
    pub fn new() -> Result<Self, String> {
        let pointer = unsafe { dna_rtd_create() };
        if pointer.is_null() {
            return Err("dc-snz.23: RTD allocation failed".into());
        }
        Ok(Self {
            state: RefCell::new(State {
                pointer,
                topics: BTreeMap::new(),
                pending_disconnects: Vec::new(),
                declarations: BTreeMap::new(),
                next: 1,
                next_transport: 1,
                transports: BTreeMap::new(),
                updates: Vec::new(),
                closed: false,
                failure: None,
                close_failure: None,
                activated: false,
            }),
        })
    }
    /// Owner polls between activations. Inputs still require CalcHost.update_rtd admission.
    pub fn poll(&self) -> Result<Vec<RtdInput>, String> {
        let mut state = self
            .state
            .try_borrow_mut()
            .map_err(|_| "dc-snz.23: reentrant RTD poll")?;
        checked(
            unsafe { dna_rtd_poll(state.pointer, publish, (&mut *state as *mut State).cast()) },
            "refresh",
        )?;
        if let Some(why) = state.failure.take() {
            return Err(why);
        }
        Ok(std::mem::take(&mut state.updates))
    }
    /// Retire and disconnect immediately when no replacement evaluation follows.
    pub fn disconnect_caller(
        &self,
        providers: &BTreeSet<RtdProvider>,
        caller: CellKey,
    ) -> Result<Vec<(RtdProvider, u64)>, String> {
        let retired = self.begin_replacement(providers, caller)?;
        self.finish_replacement(providers, caller)?;
        Ok(retired)
    }
    /// End the bounded replacement attempt, including a constant or failed replacement.
    pub fn finish_replacement(
        &self,
        providers: &BTreeSet<RtdProvider>,
        caller: CellKey,
    ) -> Result<(), String> {
        let mut state = self
            .state
            .try_borrow_mut()
            .map_err(|_| "dc-snz.23: reentrant RTD replacement completion")?;
        let mut index = 0;
        while index < state.pending_disconnects.len() {
            let pending = &state.pending_disconnects[index];
            if pending.caller == caller && providers.contains(&pending.provider) {
                let transport = pending.transport;
                checked(
                    unsafe { dna_rtd_disconnect(state.pointer, transport) },
                    "replacement disconnect",
                )?;
                state.pending_disconnects.remove(index);
            } else {
                index += 1;
            }
        }
        Self::disconnect_inactive(&mut state)
    }
    /// Retire logical inputs after the edit commits. Finish the replacement after its
    /// evaluation attempt ends, so a new native topic can connect before the old one leaves.
    pub fn begin_replacement(
        &self,
        providers: &BTreeSet<RtdProvider>,
        caller: CellKey,
    ) -> Result<Vec<(RtdProvider, u64)>, String> {
        let mut state = self
            .state
            .try_borrow_mut()
            .map_err(|_| "dc-snz.23: reentrant RTD disconnect")?;
        state
            .declarations
            .retain(|_, call| call.caller != caller || !providers.contains(&call.provider));
        let ids = state
            .topics
            .iter_mut()
            .filter_map(|(&id, topic)| {
                if providers.contains(&topic.provider) {
                    topic.callers.remove(&caller);
                    topic.active_callers.remove(&caller);
                }
                topic.callers.is_empty().then_some(id)
            })
            .collect::<Vec<_>>();
        let mut retired = Vec::new();
        for id in ids {
            let topic = state.topics.remove(&id).unwrap();
            state.transports.remove(&topic.transport);
            if topic.connected {
                state.pending_disconnects.push(PendingDisconnect {
                    provider: topic.provider,
                    caller,
                    transport: topic.transport,
                });
            }
            state.updates.retain(|value| value.subscription != id);
            retired.push((topic.provider, id));
        }
        Ok(retired)
    }
    /// Reconcile only after this cell has a successful producing certificate and
    /// no live evaluation. Reused certificates keep their recorded declarations;
    /// aborted or skipped native calls cannot keep a transport connected.
    /// A completed cached call can renew an existing transport through an exact
    /// admitted input read in that same producing certificate.
    pub fn reconcile_evaluation(
        &self,
        providers: &BTreeSet<RtdProvider>,
        caller: CellKey,
        invocations: &BTreeSet<u64>,
        inputs: &BTreeSet<calc_xll_host::RtdInputKey>,
    ) -> Result<(), String> {
        let mut state = self
            .state
            .try_borrow_mut()
            .map_err(|_| "dc-snz.23: reentrant RTD evaluation reconciliation")?;
        state.declarations.retain(|id, call| {
            call.caller != caller || !providers.contains(&call.provider) || invocations.contains(id)
        });
        let declared: BTreeSet<_> = state
            .declarations
            .values()
            .filter(|call| call.caller == caller && providers.contains(&call.provider))
            .flat_map(|call| call.subscriptions.iter().copied())
            .chain(inputs.iter().filter_map(|read| {
                // Cached native results can use an admitted RTD read without
                // making xlfRtd again. Only a completed call retained by this
                // producing certificate can renew that provider's transport.
                state
                    .declarations
                    .values()
                    .any(|call| call.caller == caller && call.provider == read.callable)
                    .then_some(read.subscription)
            }))
            .collect();
        for (&id, topic) in &mut state.topics {
            if providers.contains(&topic.provider) && !declared.contains(&id) {
                topic.active_callers.remove(&caller);
            }
        }
        Self::disconnect_inactive(&mut state)
    }
    pub fn subscriptions(&self) -> usize {
        self.state.borrow().topics.len()
    }
    fn disconnect_inactive(state: &mut State) -> Result<(), String> {
        let ids: Vec<_> = state
            .topics
            .iter()
            .filter_map(|(&id, topic)| {
                (topic.connected && topic.active_callers.is_empty()).then_some(id)
            })
            .collect();
        for id in ids {
            checked(
                unsafe { dna_rtd_disconnect(state.pointer, state.topics[&id].transport) },
                "disconnect",
            )?;
            let topic = state.topics.get_mut(&id).unwrap();
            topic.connected = false;
            let transport = topic.transport;
            state.transports.remove(&transport);
        }
        Ok(())
    }
}
impl RtdService for RtdClient {
    fn subscriptions(&self, provider: RtdProvider, caller: CellKey) -> Vec<u64> {
        self.state
            .borrow()
            .topics
            .iter()
            .filter_map(|(&id, topic)| {
                (topic.provider == provider && topic.active_callers.contains(&caller)).then_some(id)
            })
            .collect()
    }
    fn has_live_servers(&self) -> bool {
        let state = self.state.borrow();
        state.activated && (!state.closed || state.close_failure.is_some())
    }
    fn finish_call(
        &self,
        provider: RtdProvider,
        caller: CellKey,
        invocation: u64,
        declared: &[u64],
    ) -> Result<(), String> {
        let mut state = self
            .state
            .try_borrow_mut()
            .map_err(|_| "dc-snz.23: reentrant RTD declaration reconciliation")?;
        state.declarations.insert(
            invocation,
            Declaration {
                provider,
                caller,
                subscriptions: declared.iter().copied().collect(),
            },
        );
        Ok(())
    }

    fn subscribe(
        &self,
        provider: RtdProvider,
        caller: CellKey,
        progid: &str,
        strings: &[String],
    ) -> Result<u64, String> {
        let mut state = self
            .state
            .try_borrow_mut()
            .map_err(|_| "dc-snz.23: reentrant RTD subscription")?;
        if state.closed {
            return Err("dc-snz.23: RTD client is closed".into());
        }
        let existing = state.topics.iter().find_map(|(&id, topic)| {
            (topic.provider == provider && topic.progid == progid && topic.strings == strings)
                .then_some(id)
        });
        if let Some(id) = existing {
            let topic = state.topics.get_mut(&id).unwrap();
            if topic.connected {
                topic.callers.insert(caller);
                topic.active_callers.insert(caller);
                return Ok(id);
            }
        }
        // A completed transport can reconnect while an older producing input is
        // still leased. Keep its logical ID and advance, never rewind, its cut.
        let (id, sequence) = if let Some(id) = existing {
            (
                id,
                state.topics[&id]
                    .sequence
                    .checked_add(1)
                    .ok_or("dc-snz.23: RTD sequence exhausted")?,
            )
        } else {
            let id = state.next;
            state.next = state
                .next
                .checked_add(1)
                .filter(|n| *n <= i32::MAX as u64)
                .ok_or("dc-snz.23: RTD subscription identity exhausted")?;
            (id, 1)
        };
        let transport = state.next_transport;
        state.next_transport = state
            .next_transport
            .checked_add(1)
            .ok_or("dc-snz.23: RTD transport identity exhausted")?;
        let progid_w = wide(progid)?;
        let strings_w = strings
            .iter()
            .map(|s| wide(s))
            .collect::<Result<Vec<_>, _>>()?;
        let pointers = strings_w.iter().map(|s| s.as_ptr()).collect::<Vec<_>>();
        let mut initial = Value {
            kind: 0,
            number: 0.,
            text: std::ptr::null(),
            length: 0,
        };
        state.activated = true;
        let result = unsafe {
            dna_rtd_connect(
                state.pointer,
                progid_w.as_ptr(),
                transport,
                pointers.as_ptr(),
                pointers.len(),
                &mut initial,
            )
        };
        let value = if result >= 0 {
            unsafe { decode(&initial) }
        } else {
            Err(format!("dc-snz.23: RTD connect: {result:#x}"))
        };
        unsafe { dna_rtd_value_clear(&mut initial) };
        let value = match value {
            Ok(value) => value,
            Err(why) => {
                if result >= 0 {
                    unsafe { dna_rtd_disconnect(state.pointer, transport) };
                }
                return Err(why);
            }
        };
        let topic = state.topics.entry(id).or_insert_with(|| Topic {
            provider,
            progid: progid.into(),
            strings: strings.to_vec(),
            callers: BTreeSet::new(),
            active_callers: BTreeSet::new(),
            connected: false,
            transport,
            sequence: 0,
        });
        topic.callers.insert(caller);
        topic.active_callers.insert(caller);
        topic.connected = true;
        topic.transport = transport;
        topic.sequence = sequence;
        state.transports.insert(transport, id);
        state.updates.push(RtdInput {
            provider,
            subscription: id,
            sequence,
            value,
        });
        Ok(id)
    }
    fn close(&self) -> Result<(), String> {
        let mut state = self
            .state
            .try_borrow_mut()
            .map_err(|_| "dc-snz.23: reentrant RTD close")?;
        if state.closed {
            return state.close_failure.clone().map_or(Ok(()), Err);
        }
        state.closed = true;
        let result = unsafe { dna_rtd_close(state.pointer) };
        if let Err(why) = checked(result, "terminate") {
            state.close_failure = Some(why.clone());
            return Err(why);
        }
        state.topics.clear();
        state.transports.clear();
        state.declarations.clear();
        state.pending_disconnects.clear();
        state.updates.clear();
        checked(result, "terminate")
    }
}
unsafe extern "system" fn publish(context: *mut c_void, id: i32, value: *const Value) -> i32 {
    let state = unsafe { &mut *context.cast::<State>() };
    let Some(&subscription) = state.transports.get(&id) else {
        return 0;
    };
    let Some(topic) = state.topics.get_mut(&subscription) else {
        return 0;
    };
    let value = match unsafe { decode(&*value) } {
        Ok(value) => value,
        Err(why) => {
            state.failure = Some(why);
            return 0x80020005u32 as i32;
        }
    };
    let Some(sequence) = topic.sequence.checked_add(1) else {
        state.failure = Some("dc-snz.23: RTD sequence exhausted".into());
        return 0x80004005u32 as i32;
    };
    topic.sequence = sequence;
    state.updates.push(RtdInput {
        provider: topic.provider,
        subscription,
        sequence,
        value,
    });
    0
}
impl Drop for RtdClient {
    fn drop(&mut self) {
        if self.close().is_ok() {
            unsafe { dna_rtd_destroy(self.state.get_mut().pointer) };
        }
    }
}

/// Native-only: runs the COM client's disconnect/reconnect and queued SAFEARRAY refresh route.
#[test]
fn obsolete_transport_updates_cannot_reach_a_reconnected_topic() {
    unsafe extern "C" {
        fn dna_rtd_test_obsolete_transport() -> i32;
    }
    let status = unsafe { dna_rtd_test_obsolete_transport() };
    assert_eq!(status, 0, "queued obsolete RTD transport: {status:#x}");
}

#[cfg(test)]
unsafe extern "C" {
    fn dna_rtd_test_attach_queued(client: *mut c_void) -> i32;
    fn dna_rtd_test_event(
        client: *mut c_void,
        index: i32,
        method: *mut i32,
        topic: *mut i32,
    ) -> i32;
}
#[cfg(test)]
fn queued_events(client: &RtdClient) -> Vec<(i32, i32)> {
    let pointer = client.state.borrow().pointer;
    let mut events = Vec::new();
    for index in 0..16 {
        let (mut method, mut topic) = (0, 0);
        let code = unsafe { dna_rtd_test_event(pointer, index, &mut method, &mut topic) };
        if code == 1 {
            return events;
        }
        assert_eq!(code, 0);
        events.push((method, topic));
    }
    panic!("controlled RTD event log exceeded its bound")
}
/// Native-only: drives the real client against its existing injected COM fixture.
#[test]
fn replacement_connects_before_retired_transport_disconnects() {
    let client = RtdClient::new().unwrap();
    assert_eq!(
        unsafe { dna_rtd_test_attach_queued(client.state.borrow().pointer) },
        0
    );
    let provider = RtdProvider {
        table: 1,
        slot: 0,
        generation: 1,
    };
    let providers = BTreeSet::from([provider]);
    let caller = CellKey::new(calc_value::SheetId::from_parts(0, 0), 1, 1).unwrap();
    let old = client
        .subscribe(provider, caller, "fixture", &["old".into()])
        .unwrap();
    assert_eq!(
        client.begin_replacement(&providers, caller).unwrap(),
        vec![(provider, old)]
    );
    assert_eq!(client.subscriptions(), 0);
    assert_eq!(queued_events(&client), vec![(1, 1)]);
    let next = client
        .subscribe(provider, caller, "fixture", &["new".into()])
        .unwrap();
    assert_ne!(old, next);
    client.finish_replacement(&providers, caller).unwrap();
    assert_eq!(queued_events(&client), vec![(1, 1), (1, 2), (2, 1)]);
    let updates = client.poll().unwrap();
    assert_eq!(updates.len(), 1);
    assert_eq!(updates[0].subscription, next);
    assert_eq!(updates[0].sequence, 1);
    assert_eq!(updates[0].value, CalcValue::number(22.));
    assert!(client.state.borrow().pending_disconnects.is_empty());
    client.disconnect_caller(&providers, caller).unwrap();
    assert_eq!(client.subscriptions(), 0);
    assert!(queued_events(&client).is_empty());
    client.close().unwrap();
}
/// Native-only: checks native transport ownership shared by two callers during replacement.
#[test]
fn replacement_preserves_another_callers_topic_lease() {
    let client = RtdClient::new().unwrap();
    assert_eq!(
        unsafe { dna_rtd_test_attach_queued(client.state.borrow().pointer) },
        0
    );
    let provider = RtdProvider {
        table: 1,
        slot: 0,
        generation: 1,
    };
    let providers = BTreeSet::from([provider]);
    let sheet = calc_value::SheetId::from_parts(0, 0);
    let first = CellKey::new(sheet, 1, 1).unwrap();
    let other = CellKey::new(sheet, 1, 2).unwrap();
    let old = client
        .subscribe(provider, first, "fixture", &["old".into()])
        .unwrap();
    assert_eq!(
        client
            .subscribe(provider, other, "fixture", &["old".into()])
            .unwrap(),
        old
    );
    assert!(
        client
            .begin_replacement(&providers, first)
            .unwrap()
            .is_empty()
    );
    client
        .subscribe(provider, first, "fixture", &["new".into()])
        .unwrap();
    client.finish_replacement(&providers, first).unwrap();
    assert_eq!(queued_events(&client), vec![(1, 1), (1, 2)]);
    assert_eq!(
        client.disconnect_caller(&providers, other).unwrap(),
        vec![(provider, old)]
    );
    assert_eq!(queued_events(&client), vec![(1, 1), (1, 2), (2, 1)]);
    client.close().unwrap();
}

/// Native-only: a completed cached UDF keeps only its producing certificate's admitted RTD read.
#[test]
fn cached_call_renews_only_a_certified_rtd_read() {
    let client = RtdClient::new().unwrap();
    assert_eq!(
        unsafe { dna_rtd_test_attach_queued(client.state.borrow().pointer) },
        0
    );
    let provider = RtdProvider {
        table: 1,
        slot: 0,
        generation: 1,
    };
    let providers = BTreeSet::from([provider]);
    let sheet = calc_value::SheetId::from_parts(0, 0);
    let caller = CellKey::new(sheet, 1, 1).unwrap();
    let sibling = CellKey::new(sheet, 1, 2).unwrap();
    let topic = client
        .subscribe(provider, caller, "fixture", &["cached".into()])
        .unwrap();
    let other = client
        .subscribe(provider, sibling, "fixture", &["sibling".into()])
        .unwrap();
    let input = calc_xll_host::RtdInputKey {
        callable: provider,
        subscription: topic,
    };
    client.finish_call(provider, caller, 100, &[topic]).unwrap();
    client
        .reconcile_evaluation(
            &providers,
            caller,
            &BTreeSet::from([100]),
            &BTreeSet::from([input]),
        )
        .unwrap();
    // This native call returned its cached result without invoking xlfRtd again.
    client.finish_call(provider, caller, 101, &[]).unwrap();
    client
        .reconcile_evaluation(
            &providers,
            caller,
            &BTreeSet::from([101]),
            &BTreeSet::from([input]),
        )
        .unwrap();
    assert!(client.state.borrow().topics[&topic].connected);
    assert_eq!(queued_events(&client), vec![(1, 1), (1, 2)]);
    // A completed call with no producing read cannot keep the transport alive.
    client.finish_call(provider, caller, 102, &[]).unwrap();
    client
        .reconcile_evaluation(&providers, caller, &BTreeSet::from([102]), &BTreeSet::new())
        .unwrap();
    assert!(!client.state.borrow().topics[&topic].connected);
    assert!(client.state.borrow().topics[&other].connected);
    assert_eq!(queued_events(&client), vec![(1, 1), (1, 2), (2, 1)]);
    // A read alone also cannot renew an aborted/skipped call's transport.
    assert_eq!(
        client
            .subscribe(provider, caller, "fixture", &["cached".into()])
            .unwrap(),
        topic
    );
    client
        .reconcile_evaluation(
            &providers,
            caller,
            &BTreeSet::from([103]),
            &BTreeSet::from([input]),
        )
        .unwrap();
    assert!(!client.state.borrow().topics[&topic].connected);
    assert!(client.state.borrow().topics[&other].connected);
    client.close().unwrap();
}
