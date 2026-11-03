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
    sequence: u64,
}
struct State {
    pointer: *mut c_void,
    topics: BTreeMap<u64, Topic>,
    next: u64,
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
                next: 1,
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
    /// Call after an actual formula replacement, with that workbook's pinned providers.
    pub fn disconnect_caller(
        &self,
        providers: &BTreeSet<RtdProvider>,
        caller: CellKey,
    ) -> Result<Vec<(RtdProvider, u64)>, String> {
        let mut state = self
            .state
            .try_borrow_mut()
            .map_err(|_| "dc-snz.23: reentrant RTD disconnect")?;
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
        Self::disconnect_inactive(&mut state)?;
        let mut retired = Vec::new();
        for id in ids {
            let topic = state.topics.remove(&id).unwrap();
            state.updates.retain(|value| value.subscription != id);
            retired.push((topic.provider, id));
        }
        Ok(retired)
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
                unsafe { dna_rtd_disconnect(state.pointer, id as i32) },
                "disconnect",
            )?;
            state.topics.get_mut(&id).unwrap().connected = false;
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
        declared: &[u64],
    ) -> Result<(), String> {
        let mut state = self
            .state
            .try_borrow_mut()
            .map_err(|_| "dc-snz.23: reentrant RTD declaration reconciliation")?;
        for (&id, topic) in &mut state.topics {
            if topic.provider == provider && !declared.contains(&id) {
                topic.active_callers.remove(&caller);
            }
        }
        Self::disconnect_inactive(&mut state)
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
        if let Some((&id, topic)) = state.topics.iter_mut().find(|(_, topic)| {
            topic.provider == provider && topic.progid == progid && topic.strings == strings
        }) {
            if !topic.connected {
                return Err("dc-snz.23: reconnecting a completed RTD transport before retiring its input is not modelled".into());
            }
            topic.callers.insert(caller);
            topic.active_callers.insert(caller);
            return Ok(id);
        }
        let id = state.next;
        state.next = state
            .next
            .checked_add(1)
            .filter(|n| *n <= i32::MAX as u64)
            .ok_or("dc-snz.23: RTD subscription identity exhausted")?;
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
                id as i32,
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
                    unsafe { dna_rtd_disconnect(state.pointer, id as i32) };
                }
                return Err(why);
            }
        };
        state.topics.insert(
            id,
            Topic {
                provider,
                progid: progid.into(),
                strings: strings.to_vec(),
                callers: [caller].into(),
                active_callers: [caller].into(),
                connected: true,
                sequence: 1,
            },
        );
        state.updates.push(RtdInput {
            provider,
            subscription: id,
            sequence: 1,
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
        state.updates.clear();
        checked(result, "terminate")
    }
}
unsafe extern "system" fn publish(context: *mut c_void, id: i32, value: *const Value) -> i32 {
    let state = unsafe { &mut *context.cast::<State>() };
    let Some(topic) = state.topics.get_mut(&(id as u64)) else {
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
        subscription: id as u64,
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
