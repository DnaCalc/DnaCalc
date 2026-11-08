//! dc-snz.23 milestone 6: RTD inputs in a OneCalc session, with an owner connector that is a fake
//! server. The same rules calc-fml defines for every host: ConnectData's value shows at once and is
//! certified once the session admits it, a later update advances the input and the owner
//! calculates again, and a formula that stops reading a topic leaves its lease to be disconnected.
use calc_func::value::{CalcValue, CellKey};
use calc_fml::rtd::RtdDisposition;
use calc_fml::{RtdConnection, RtdConnector, RtdLease, TopicKey};
use dna_onecalc::Session;
use std::cell::RefCell;
use std::rc::Rc;

/// A server: the first connection of a topic returns 1, later reads join it.
#[derive(Debug, Default)]
struct Server {
    connected: RefCell<Vec<(TopicKey, u64)>>,
}

impl RtdConnector for Server {
    fn connect(&self, topic: &TopicKey, _caller: CellKey, _declared: bool) -> Result<RtdConnection, String> {
        let mut connected = self.connected.borrow_mut();
        if let Some((_, lease)) = connected.iter().find(|(held, _)| held == topic) {
            return Ok(RtdConnection { lease: RtdLease(*lease), provisional: None });
        }
        let lease = connected.len() as u64 + 1;
        connected.push((topic.clone(), lease));
        Ok(RtdConnection { lease: RtdLease(lease), provisional: Some((1, CalcValue::number(1.0))) })
    }
}

fn session() -> (Session, Rc<Server>) {
    let server = Rc::new(Server::default());
    let mut session = Session::new("Sheet1").unwrap();
    session.set_formula_cell("B3").unwrap();
    session.set_rtd_connector(server.clone());
    (session, server)
}

fn number(session: &mut Session, formula: &str) -> f64 {
    session.set_formula(formula);
    let calculation = session.calculate();
    let value = calculation
        .result
        .unwrap_or_else(|| panic!("{formula}: {:?}", calculation.diagnostics));
    value.as_number().unwrap_or_else(|| panic!("{formula}: {value:?}"))
}

#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn a_connection_shows_its_first_value_and_an_update_makes_the_owner_calculate_again() {
    let (mut session, _server) = session();
    // ConnectData's value shows at once, before the session has admitted anything.
    assert_eq!(number(&mut session, "=10+RTD(\"P.Server\",,\"a\")+3"), 14.0);
    let reads = session.rtd_reads().to_vec();
    assert_eq!(reads.len(), 1);
    assert_eq!((reads[0].lease, reads[0].sequence), (RtdLease(1), 1));
    // The session admitted the connection's value as sequence 1 when the evaluation ended.
    let admitted = session.take_rtd_admissions();
    assert_eq!(admitted.len(), 1);
    assert_eq!((admitted[0].sequence, admitted[0].value.clone()), (1, CalcValue::number(1.0)));
    // A RefreshData: sequence 2 with 42. The formula read that topic, so the owner calculates again.
    let topic = reads[0].topic.clone();
    assert_eq!(session.update_rtd(topic.clone(), RtdLease(1), 2, CalcValue::number(42.0)), RtdDisposition::Accepted);
    assert!(session.reads_rtd(&topic, RtdLease(1)));
    assert_eq!(number(&mut session, "=10+RTD(\"P.Server\",,\"a\")+3"), 55.0);
    assert_eq!(session.rtd_reads()[0].sequence, 2);
    // The same update again, an older one and one for another incarnation are not admitted.
    assert_eq!(session.update_rtd(topic.clone(), RtdLease(1), 2, CalcValue::number(42.0)), RtdDisposition::Duplicate);
    assert_eq!(session.update_rtd(topic.clone(), RtdLease(1), 1, CalcValue::number(1.0)), RtdDisposition::StaleSequence);
    assert_eq!(session.update_rtd(topic, RtdLease(1), 0, CalcValue::number(1.0)), RtdDisposition::InvalidValue);
}

#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn a_formula_that_stops_reading_a_topic_leaves_its_lease_to_be_disconnected() {
    let (mut session, _server) = session();
    assert_eq!(number(&mut session, "=RTD(\"P.Server\",,\"a\")"), 1.0);
    let topic = session.rtd_reads()[0].topic.clone();
    // The new formula is calculated first (it may connect the next topic), then the old one leaves.
    assert_eq!(number(&mut session, "=RTD(\"P.Server\",,\"b\")"), 1.0);
    let unread = session.take_unread_rtd();
    assert_eq!(unread, vec![(topic.clone(), RtdLease(1))]);
    assert_eq!(session.retire_rtd(topic.clone(), RtdLease(1)), RtdDisposition::Accepted);
    // A retired connection admits nothing more.
    assert_eq!(session.update_rtd(topic, RtdLease(1), 3, CalcValue::number(9.0)), RtdDisposition::Retired);
    // The topic still read is not touched.
    assert_eq!(session.rtd_reads().len(), 1);
    assert!(session.take_unread_rtd().is_empty());
}

#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn without_a_connector_rtd_is_a_typed_refusal_and_a_part_of_the_formula_changes_no_state() {
    let mut bare = Session::new("Sheet1").unwrap();
    bare.set_formula("=RTD(\"P.Server\",,\"a\")");
    let outcome = bare.calculate();
    assert!(outcome.result.is_none(), "{outcome:?}");
    // F9 on a part evaluates with the inputs but leaves the session's connection state alone.
    let (mut session, _server) = session();
    assert_eq!(number(&mut session, "=RTD(\"P.Server\",,\"a\")+1"), 2.0);
    let before = session.rtd_reads().to_vec();
    let part = session.calculate_text("=RTD(\"P.Server\",,\"zzz\")");
    assert!(part.result.is_some());
    assert_eq!(session.rtd_reads(), before.as_slice());
    assert!(session.take_unread_rtd().is_empty());
}
