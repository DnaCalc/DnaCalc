//! The OneCalc harness with RTD (milestone 6, dc-snz.23): the command-line comparison that
//! drives the formula session over the loaded add-ins' real RTD servers, the way
//! `dna-calc-desktop --engine` drives CalcHost, so the same ordered client calls can be compared
//! with Excel's. OneCalc has one formula and no dependents: a case's `dependent` is not run, and
//! the output says so (`single_formula`).
//!
//! The session admits the connection's own value as it is returned, the owner polls the RTD
//! client between turns (`RefreshData` under the throttle, never during an evaluation), admits
//! each update as the next sequence and calculates the formula again when it read that topic.
//! A replaced formula is calculated first; the connections it no longer reads are then
//! disconnected (`begin_replacement`, `finish_replacement`, `reconcile_evaluation`).
use calc_addin_host::{AddinHost, value_json};
use calc_fml::rtd::RtdDisposition;
use calc_platform_desktop::Shell;
use dna_onecalc::Session;
use std::collections::BTreeSet;
use std::time::{Duration, Instant};

fn admissions(session: &mut Session, out: &mut Vec<serde_json::Value>) {
    for admission in session.take_rtd_admissions() {
        out.push(serde_json::json!({"topic":format!("{:?}",admission.topic),"lease":admission.lease.0,"sequence":admission.sequence,"value":value_json(&admission.value),"disposition":"Accepted"}));
    }
}

fn provenance(session: &Session) -> serde_json::Value {
    serde_json::json!({
        "rtd_inputs": session.rtd_reads().iter().map(|read| serde_json::json!({"topic":format!("{:?}",read.topic),"lease":read.lease.0,"sequence":read.sequence})).collect::<Vec<_>>(),
        "completed_invocations": session.rtd_invocations().len(),
    })
}

/// Calculates the session's formula; a diagnostic is an error. The value is the calculated one.
fn calculate(session: &mut Session, formula: &str) -> Result<calc_value::CalcValue, String> {
    let calculation = session.calculate();
    if !calculation.diagnostics.is_empty() {
        return Err(format!("{formula}: {:?}", calculation.diagnostics));
    }
    calculation.result.ok_or_else(|| format!("{formula}: no calculated value"))
}

fn same(actual: &serde_json::Value, expected: &serde_json::Value) -> bool {
    actual == expected || (actual.is_number() && expected.is_number() && actual.as_f64() == expected.as_f64())
}

/// Runs `cases` (the RTD case files: `formula`, `value`, optional `rtd_wait_value`, `rtd_idle_ms`)
/// and returns the fields the verifier reads from a host's output.
pub fn run(
    shell: &Shell,
    addins: &mut AddinHost,
    session: &mut Session,
    cases: &[serde_json::Value],
) -> Result<serde_json::Value, String> {
    let rtd = addins.enable_rtd()?;
    session.set_rtd_connector(rtd.clone());
    session.set_user_functions(addins.callables()?);
    // The providers are the handles the session's live table gives the registrations, which is
    // what each call (and so each RTD declaration) carries.
    let providers: BTreeSet<calc_xll_host::RtdProvider> = addins
        .registrations()
        .iter()
        .filter_map(|r| session.bind_function(&r.name))
        .collect();
    let at = session.formula_cell();
    let mut results = Vec::new();
    let mut updates = Vec::new();
    let mut retired = Vec::new();
    for case in cases {
        let text = case["formula"].as_str().ok_or("case needs formula")?;
        // A case formula's {pid} is this process's id, as the owned Excel's is its own.
        let formula = text.replace("{pid}", &std::process::id().to_string());
        session.set_formula(&formula);
        // The formula was replaced: the connections its caller held are retired from the session's
        // ledger now and disconnected once the new formula has been calculated.
        for (topic, lease) in rtd.begin_replacement(&providers, at)? {
            let disposition = session.retire_rtd(topic.clone(), lease);
            if disposition != RtdDisposition::Accepted {
                return Err(format!("RTD disconnect was not admitted: {disposition:?}"));
            }
            retired.push(serde_json::json!({"topic":format!("{topic:?}"),"lease":lease.0,"disposition":format!("{disposition:?}")}));
        }
        addins.sheet_changed(at.row(), at.column())?;
        let mut value = calculate(session, &formula)?;
        admissions(session, &mut updates);
        let settle = |session: &Session| -> Result<(), String> {
            rtd.finish_replacement(&providers, at)?;
            rtd.reconcile_evaluation(&providers, at, session.rtd_invocations())
        };
        settle(session)?;
        let deadline = Instant::now() + Duration::from_secs(20);
        let idle = Duration::from_millis(case["rtd_idle_ms"].as_u64().unwrap_or(0));
        let mut idle_until = None;
        // The turn right after an evaluation takes only the updates that evaluation may see and
        // does not disconnect what no formula declared (the formula was just calculated, as
        // CalcHost's first turn is, whose calculation is still in flight); later turns are idle.
        let mut after_evaluation = true;
        loop {
            for input in addins.poll_rtd(std::mem::take(&mut after_evaluation))? {
                let disposition = session.update_rtd(
                    input.topic.clone(),
                    input.lease,
                    input.sequence,
                    input.value.clone(),
                );
                match disposition {
                    // The session already admitted the connection's own value as it was returned;
                    // the client's queued copy of it is the same admission.
                    RtdDisposition::Duplicate => continue,
                    RtdDisposition::Accepted => {}
                    other => return Err(format!("RTD input was not admitted: {other:?}")),
                }
                admissions(session, &mut updates);
                // The formula read this topic: the owner calculates it again.
                if session.reads_rtd(&input.topic, input.lease) {
                    value = calculate(session, &formula)?;
                    admissions(session, &mut updates);
                    settle(session)?;
                }
            }
            let complete = !rtd.has_pending_inputs()
                && case.get("rtd_wait_value").is_none_or(|expected| same(&value_json(&value), expected));
            if complete && idle_until.is_none() {
                if idle.is_zero() {
                    break;
                }
                idle_until = Some(Instant::now().checked_add(idle).ok_or("RTD idle interval overflow")?);
            }
            if let Some(until) = idle_until {
                if Instant::now() >= until {
                    if complete {
                        break;
                    }
                    return Err("RTD idle observation ended with incomplete calculation".into());
                }
            } else if Instant::now() > deadline {
                return Err("RTD completion deadline expired".into());
            }
            shell.pump_for(Duration::from_millis(2));
        }
        let idle_observed_ms = idle_until.map(|until| (Instant::now() - (until - idle)).as_millis());
        let actual = value_json(&value);
        if !case["value"].is_null() && !same(&actual, &case["value"]) {
            return Err(format!("{formula}: calculated {actual}, expected {}", case["value"]));
        }
        results.push(serde_json::json!({
            "formula": text,
            "value": actual,
            "dependent": null,
            "provenance": provenance(session),
            "idle_observed_ms": idle_observed_ms,
            "rtd_updates_through": updates.len(),
        }));
        addins.after_calculate()?;
    }
    // Connections the last formula no longer reads were reconciled above; the rest end with the
    // add-ins (`close_addins` runs ServerTerminate through the client).
    Ok(serde_json::json!({
        "single_formula": true,
        "results": results,
        "rtd_updates": updates,
        "rtd_retired": retired,
        "async_returns": [],
        "revoked_tokens": [],
        "rtd_subscriptions_after_close": 0,
    }))
}
