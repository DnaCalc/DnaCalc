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
// Worksheet numeric equality folds signed zero. Native operands must retain it,
// including inside arrays, when identifying a previous native call.
fn same_argument(left: &CalcValue, right: &CalcValue) -> bool {
    use calc_value::CoreRef;
    if left.is_excel_overflow() != right.is_excel_overflow() {
        return false;
    }
    match (left.core_ref(), right.core_ref()) {
        (CoreRef::Number(a), CoreRef::Number(b)) => a.to_bits() == b.to_bits(),
        (CoreRef::Array(a), CoreRef::Array(b)) => {
            a.shape() == b.shape()
                && a.is_boxed() == b.is_boxed()
                && a.iter_row_major()
                    .zip(b.iter_row_major())
                    .all(|(a, b)| same_argument(a, b))
        }
        _ => left == right,
    }
}
fn has_reference(value: &CalcValue) -> bool {
    value.as_reference().is_some()
        || value
            .as_array()
            .is_some_and(|array| array.iter_row_major().any(has_reference))
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
    last_value: CalcValue,
}
struct Declaration {
    provider: RtdProvider,
    caller: CellKey,
    arguments: Vec<CalcValue>,
    subscriptions: BTreeSet<u64>,
    inputs: BTreeSet<u64>,
}
struct PendingDisconnect {
    provider: RtdProvider,
    caller: CellKey,
    subscription: u64,
    topic: Topic,
}
struct State {
    pointer: *mut c_void,
    topics: BTreeMap<u64, Topic>,
    pending_disconnects: Vec<PendingDisconnect>,
    declarations: BTreeMap<u64, Declaration>,
    replacement_calls: Vec<Declaration>,
    renewed_inputs: BTreeMap<u64, u64>,
    next: u64,
    next_transport: i32,
    transports: BTreeMap<i32, u64>,
    updates: Vec<RtdInput>,
    initial_values: BTreeMap<u64, (u64, CalcValue)>,
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
                replacement_calls: Vec::new(),
                renewed_inputs: BTreeMap::new(),
                next: 1,
                next_transport: 1,
                transports: BTreeMap::new(),
                updates: Vec::new(),
                initial_values: BTreeMap::new(),
                closed: false,
                failure: None,
                close_failure: None,
                activated: false,
            }),
        })
    }
    /// Owner polls between activations. Inputs still require CalcHost.update_rtd admission.
    pub fn poll(&self) -> Result<Vec<RtdInput>, String> {
        self.poll_inputs(true)
    }
    /// A calculation can poll signals without destroying a completed native cache
    /// before its consumer activation has read the newly admitted input.
    pub fn poll_during_evaluation(&self) -> Result<Vec<RtdInput>, String> {
        self.poll_inputs(false)
    }
    fn poll_inputs(&self, retire_unused: bool) -> Result<Vec<RtdInput>, String> {
        let mut state = self
            .state
            .try_borrow_mut()
            .map_err(|_| "dc-snz.23: reentrant RTD poll")?;
        // Retire unused native transports at the next owner turn. An immediate
        // formula replacement can first connect its next topic. An idle owner
        // still disconnects completed generators before polling their servers.
        if retire_unused {
            Self::disconnect_inactive(&mut state)?;
        }
        checked(
            unsafe { dna_rtd_poll(state.pointer, publish, (&mut *state as *mut State).cast()) },
            "refresh",
        )?;
        if let Some(why) = state.failure.take() {
            return Err(why);
        }
        state.initial_values.clear();
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
                let transport = pending.topic.transport;
                checked(
                    unsafe { dna_rtd_disconnect(state.pointer, transport) },
                    "replacement disconnect",
                )?;
                state.pending_disconnects.remove(index);
            } else {
                index += 1;
            }
        }
        state
            .replacement_calls
            .retain(|call| call.caller != caller || !providers.contains(&call.provider));
        let retained: BTreeSet<_> = state
            .replacement_calls
            .iter()
            .flat_map(|call| call.inputs.iter().copied())
            .collect();
        state.renewed_inputs.retain(|old, _| retained.contains(old));
        Ok(())
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
            .replacement_calls
            .retain(|call| call.caller != caller || !providers.contains(&call.provider));
        let calls = state
            .declarations
            .iter()
            .filter_map(|(&id, call)| {
                (call.caller == caller && providers.contains(&call.provider)).then_some(id)
            })
            .collect::<Vec<_>>();
        for id in calls {
            let call = state.declarations.remove(&id).unwrap();
            state.replacement_calls.push(call);
        }
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
            let provider = topic.provider;
            state.transports.remove(&topic.transport);
            if topic.connected {
                state.pending_disconnects.push(PendingDisconnect {
                    provider: topic.provider,
                    caller,
                    subscription: id,
                    topic,
                });
            }
            state.updates.retain(|value| value.subscription != id);
            state.initial_values.remove(&id);
            retired.push((provider, id));
        }
        Ok(retired)
    }
    /// Reconcile only after this cell has a successful producing certificate and
    /// no live evaluation. Reused certificates keep their recorded declarations;
    /// aborted or skipped native calls cannot keep a transport connected.
    pub fn reconcile_evaluation(
        &self,
        providers: &BTreeSet<RtdProvider>,
        caller: CellKey,
        invocations: &BTreeSet<u64>,
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
            .collect();
        for (&id, topic) in &mut state.topics {
            if providers.contains(&topic.provider) && !declared.contains(&id) {
                topic.active_callers.remove(&caller);
            }
        }
        Ok(())
    }
    pub fn subscriptions(&self) -> usize {
        self.state.borrow().topics.len()
    }
    /// Observed native inputs the owner must admit before declaring calculation complete.
    pub fn has_pending_inputs(&self) -> bool {
        !self.state.borrow().updates.is_empty()
    }
    fn disconnect_inactive(state: &mut State) -> Result<(), String> {
        let retained: BTreeSet<_> = state
            .replacement_calls
            .iter()
            .flat_map(|call| call.inputs.iter().copied())
            .map(|id| state.renewed_inputs.get(&id).copied().unwrap_or(id))
            .chain(state.updates.iter().map(|input| input.subscription))
            .collect();
        let ids: Vec<_> = state
            .topics
            .iter()
            .filter_map(|(&id, topic)| {
                (topic.connected && topic.active_callers.is_empty() && !retained.contains(&id))
                    .then_some(id)
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
    fn initial_value(&self, provider: RtdProvider, subscription: u64) -> Option<(u64, CalcValue)> {
        let state = self.state.borrow();
        (state.topics.get(&subscription)?.provider == provider)
            .then(|| state.initial_values.get(&subscription).cloned())
            .flatten()
    }
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
        arguments: &[CalcValue],
        declared: &[u64],
        inputs: &[u64],
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
                arguments: arguments.to_vec(),
                subscriptions: declared.iter().copied().collect(),
                inputs: inputs.iter().copied().collect(),
            },
        );
        Ok(())
    }

    fn cached_call_inputs(
        &self,
        provider: RtdProvider,
        caller: CellKey,
        arguments: &[CalcValue],
    ) -> Result<Vec<u64>, String> {
        let mut state = self
            .state
            .try_borrow_mut()
            .map_err(|_| "dc-snz.23: reentrant cached RTD input renewal")?;
        if state.closed {
            return Err("dc-snz.23: RTD client is closed".into());
        }
        let matching: Vec<_> = state
            .declarations
            .values()
            .chain(state.replacement_calls.iter())
            .filter(|call| {
                call.provider == provider
                    && call.arguments.len() == arguments.len()
                    && call
                        .arguments
                        .iter()
                        .zip(arguments)
                        .all(|(a, b)| same_argument(a, b))
            })
            .collect();
        let own = matching
            .iter()
            .any(|call| call.caller == caller && !call.inputs.is_empty());
        // A framework can return a shared native cache before this caller ever
        // receives xlfRtd. Keep the observed sibling inputs conservatively when
        // no own declaration exists; never infer an input from a UDF answer.
        let previous: BTreeSet<_> = matching
            .into_iter()
            .filter(|call| !own || call.caller == caller)
            .flat_map(|call| call.inputs.iter().copied())
            .collect();
        if !previous.is_empty() && arguments.iter().any(has_reference) {
            return Err(
                "dc-snz.23: cached RTD reference-argument dependencies are not modelled".into(),
            );
        }
        let mut inputs = BTreeSet::new();
        for old in previous {
            let current = state.renewed_inputs.get(&old).copied().unwrap_or(old);
            if let Some(topic) = state.topics.get_mut(&current) {
                if topic.provider != provider {
                    return Err("dc-snz.23: cached RTD input belongs to another provider".into());
                }
                // Disconnection does not retire an admitted logical input.
                topic.callers.insert(caller);
                inputs.insert(current);
                continue;
            }
            let Some(index) = state
                .pending_disconnects
                .iter()
                .position(|pending| pending.subscription == old && pending.provider == provider)
            else {
                return Err(
                    "dc-snz.23: retired cached RTD input has no live replacement lease".into(),
                );
            };
            let id = state.next;
            state.next = state
                .next
                .checked_add(1)
                .filter(|n| *n <= i32::MAX as u64)
                .ok_or("dc-snz.23: RTD subscription identity exhausted")?;
            // This is a new logical key, whose first admitted cut is1. The old
            // key stays retired; reconnecting the same live key still advances.
            let sequence = 1;
            let mut topic = state.pending_disconnects.remove(index).topic;
            topic.callers.insert(caller);
            topic.sequence = sequence;
            let value = topic.last_value.clone();
            // Transfer the still-live native connection, never a retired native
            // transport. Queued old logical inputs were removed by the edit.
            // Its last observed signal gets a fresh logical lease and must pass
            // the same owner admission as ConnectData and RefreshData inputs.
            state.transports.insert(topic.transport, id);
            state.topics.insert(id, topic);
            state.renewed_inputs.insert(old, id);
            state.updates.push(RtdInput {
                provider,
                subscription: id,
                sequence,
                value,
            });
            inputs.insert(id);
        }
        Ok(inputs.into_iter().collect())
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
            last_value: CalcValue::empty(),
        });
        topic.callers.insert(caller);
        topic.active_callers.insert(caller);
        topic.connected = true;
        topic.transport = transport;
        topic.sequence = sequence;
        topic.last_value = value.clone();
        state.transports.insert(transport, id);
        state.initial_values.insert(id, (sequence, value.clone()));
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
        state.replacement_calls.clear();
        state.renewed_inputs.clear();
        state.pending_disconnects.clear();
        state.updates.clear();
        state.initial_values.clear();
        checked(result, "terminate")
    }
}
unsafe extern "system" fn publish(context: *mut c_void, id: i32, value: *const Value) -> i32 {
    let state = unsafe { &mut *context.cast::<State>() };
    let subscription = state.transports.get(&id).copied();
    let pending = state
        .pending_disconnects
        .iter()
        .position(|pending| pending.topic.transport == id);
    let topic = if let Some(subscription) = subscription {
        state.topics.get_mut(&subscription)
    } else {
        pending.map(|index| &mut state.pending_disconnects[index].topic)
    };
    let Some(topic) = topic else {
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
    topic.last_value = value.clone();
    // A live connection held for replacement can still deliver a signal. Keep
    // the observation, but never publish it to the retired logical subscription.
    let Some(subscription) = subscription else {
        return 0;
    };
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

/// Native-only: observes real COM calls across a completed call and its replacement.
#[test]
fn completed_call_defers_disconnect_until_replacement_can_connect() {
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
    client
        .finish_call(provider, caller, 100, &[], &[], &[])
        .unwrap();
    client
        .reconcile_evaluation(&providers, caller, &BTreeSet::from([100]))
        .unwrap();
    assert_eq!(queued_events(&client), vec![(1, 1)]);
    assert_eq!(
        client.begin_replacement(&providers, caller).unwrap(),
        vec![(provider, old)]
    );
    client
        .subscribe(provider, caller, "fixture", &["next".into()])
        .unwrap();
    client.finish_replacement(&providers, caller).unwrap();
    assert_eq!(queued_events(&client), vec![(1, 1), (1, 2), (2, 1)]);
    client.close().unwrap();
}

/// Native-only: renews one completed call's live COM signal after both logical callers retire.
#[test]
fn cached_replacement_renews_one_signal_for_main_and_dependent_reads() {
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
    let main = CellKey::new(sheet, 1, 1).unwrap();
    let dependent = CellKey::new(sheet, 1, 2).unwrap();
    let arguments = [CalcValue::text(ExcelText::from_utf16_code_units(
        "shared".encode_utf16().collect(),
    ))];
    let old = client
        .subscribe(provider, main, "fixture", &["shared".into()])
        .unwrap();
    assert_eq!(
        client
            .subscribe(provider, dependent, "fixture", &["shared".into()])
            .unwrap(),
        old
    );
    client
        .finish_call(provider, main, 100, &arguments, &[], &[old])
        .unwrap();
    client
        .finish_call(provider, dependent, 101, &arguments, &[], &[old])
        .unwrap();
    client
        .reconcile_evaluation(&providers, main, &BTreeSet::from([100]))
        .unwrap();
    client
        .reconcile_evaluation(&providers, dependent, &BTreeSet::from([101]))
        .unwrap();
    assert!(
        client
            .begin_replacement(&providers, dependent)
            .unwrap()
            .is_empty()
    );
    assert_eq!(
        client.begin_replacement(&providers, main).unwrap(),
        vec![(provider, old)]
    );
    assert!(
        client
            .cached_call_inputs(
                provider,
                main,
                &[CalcValue::text(ExcelText::from_utf16_code_units(
                    "different".encode_utf16().collect()
                ))]
            )
            .unwrap()
            .is_empty()
    );
    assert_eq!(client.subscriptions(), 0);
    // A signal arriving during the handoff must not enter the old logical queue.
    let native_value = Value {
        kind: 1,
        number: 42.,
        text: std::ptr::null(),
        length: 0,
    };
    {
        let mut state = client.state.borrow_mut();
        assert_eq!(
            unsafe { publish((&mut *state as *mut State).cast(), 1, &native_value) },
            0
        );
        assert!(state.updates.is_empty());
    }
    let renewed = client
        .cached_call_inputs(provider, main, &arguments)
        .unwrap();
    assert_eq!(renewed.len(), 1);
    assert_ne!(renewed[0], old);
    assert_eq!(
        client
            .cached_call_inputs(provider, dependent, &arguments)
            .unwrap(),
        renewed
    );
    assert_eq!(queued_events(&client), vec![(1, 1)]);
    {
        let state = client.state.borrow();
        let queued = &state.updates;
        assert_eq!(queued.len(), 1);
        assert_eq!(queued[0].subscription, renewed[0]);
        assert_eq!(queued[0].sequence, 1);
        assert_eq!(queued[0].value, CalcValue::number(42.));
    }
    // Only these successful replacement calls can be used by the next edit.
    client
        .finish_call(provider, main, 102, &arguments, &[], &renewed)
        .unwrap();
    client
        .finish_call(provider, dependent, 103, &arguments, &[], &renewed)
        .unwrap();
    client.finish_replacement(&providers, main).unwrap();
    client.finish_replacement(&providers, dependent).unwrap();
    client
        .reconcile_evaluation(&providers, main, &BTreeSet::new())
        .unwrap();
    // A new call can still conservatively adopt the sibling's observed input.
    assert_eq!(
        client
            .cached_call_inputs(provider, main, &arguments)
            .unwrap(),
        renewed
    );
    // Completed generators do not remain connected throughout an idle owner turn.
    assert!(client.has_pending_inputs());
    let delivered = client.poll().unwrap();
    assert_eq!(delivered.len(), 1);
    assert_eq!(delivered[0].subscription, renewed[0]);
    assert_eq!(delivered[0].sequence, 1);
    assert!(!client.has_pending_inputs());
    assert!(client.state.borrow().topics[&renewed[0]].connected);
    client.poll_during_evaluation().unwrap();
    assert!(client.state.borrow().topics[&renewed[0]].connected);
    client.poll().unwrap();
    assert!(!client.state.borrow().topics[&renewed[0]].connected);
    client.close().unwrap();
}

/// Native-only: keeps admitted inputs after native disconnect and refuses an expired handoff.
#[test]
fn cached_reads_survive_disconnect_but_not_unrenewable_retirement() {
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
    let arguments = [CalcValue::number(0.)];
    let topic = client
        .subscribe(provider, caller, "fixture", &["cached".into()])
        .unwrap();
    assert_eq!(client.poll().unwrap().len(), 1);
    client
        .finish_call(provider, caller, 100, &arguments, &[], &[topic])
        .unwrap();
    client
        .reconcile_evaluation(&providers, caller, &BTreeSet::from([100]))
        .unwrap();
    client.poll().unwrap();
    assert!(!client.state.borrow().topics[&topic].connected);
    assert_eq!(
        client
            .cached_call_inputs(provider, caller, &arguments)
            .unwrap(),
        vec![topic]
    );
    assert!(
        client
            .cached_call_inputs(provider, caller, &[CalcValue::number(-0.)])
            .unwrap()
            .is_empty()
    );
    client.begin_replacement(&providers, caller).unwrap();
    assert!(
        client
            .cached_call_inputs(provider, caller, &arguments)
            .unwrap_err()
            .starts_with("dc-snz.23:")
    );
    client.finish_replacement(&providers, caller).unwrap();
    client.close().unwrap();
}

/// Native-only: checks call keys and typed refusal without creating any COM registration.
#[test]
fn cached_call_keys_preserve_number_bits_and_refuse_reference_recovery() {
    let positive = CalcValue::number(0.);
    let negative = CalcValue::number(-0.);
    assert!(!same_argument(&positive, &negative));
    let array = |value| CalcValue::array(calc_value::CalcArray::from_scalar(value));
    assert!(!same_argument(&array(positive.clone()), &array(negative)));
    assert!(same_argument(&array(positive.clone()), &array(positive)));
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
    let caller = CellKey::new(calc_value::SheetId::from_parts(0, 0), 1, 1).unwrap();
    let arguments = [CalcValue::reference(calc_value::ReferenceLike::cell(
        caller,
    ))];
    let topic = client
        .subscribe(provider, caller, "fixture", &["cached".into()])
        .unwrap();
    client
        .finish_call(provider, caller, 100, &arguments, &[topic], &[topic])
        .unwrap();
    assert!(
        client
            .cached_call_inputs(provider, caller, &arguments)
            .unwrap_err()
            .starts_with("dc-snz.23:")
    );
    client.close().unwrap();
}

/// Native-only: observes the real in-process COM fixture's initial value and shared lease.
#[test]
fn first_cache_only_sibling_keeps_the_observed_input() {
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
    let sheet = calc_value::SheetId::from_parts(0, 0);
    let main = CellKey::new(sheet, 1, 1).unwrap();
    let sibling = CellKey::new(sheet, 1, 2).unwrap();
    let arguments = [CalcValue::number(42.)];
    let id = client
        .subscribe(provider, main, "fixture", &["shared".into()])
        .unwrap();
    assert_eq!(
        client.initial_value(provider, id),
        Some((1, CalcValue::number(11.)))
    );
    let different = RtdProvider {
        slot: 1,
        ..provider
    };
    assert_eq!(client.initial_value(different, id), None);
    let observed = client.poll().unwrap();
    assert_eq!(observed.len(), 1);
    assert_eq!((observed[0].subscription, observed[0].sequence), (id, 1));
    assert_eq!(observed[0].value, CalcValue::number(11.));
    assert_eq!(client.initial_value(provider, id), None);
    client
        .finish_call(provider, main, 100, &arguments, &[id], &[id])
        .unwrap();
    assert_eq!(
        client
            .cached_call_inputs(provider, sibling, &arguments)
            .unwrap(),
        vec![id]
    );
    assert!(
        client
            .cached_call_inputs(different, sibling, &arguments)
            .unwrap()
            .is_empty()
    );
    assert!(
        client
            .cached_call_inputs(provider, sibling, &[CalcValue::number(43.)])
            .unwrap()
            .is_empty()
    );
    client
        .finish_call(provider, sibling, 101, &arguments, &[], &[id])
        .unwrap();
    let providers = BTreeSet::from([provider]);
    client
        .reconcile_evaluation(&providers, main, &BTreeSet::from([100]))
        .unwrap();
    client
        .reconcile_evaluation(&providers, sibling, &BTreeSet::from([101]))
        .unwrap();
    assert!(
        client
            .begin_replacement(&providers, main)
            .unwrap()
            .is_empty()
    );
    assert_eq!(
        client
            .cached_call_inputs(provider, sibling, &arguments)
            .unwrap(),
        vec![id]
    );
    client.close().unwrap();
}
