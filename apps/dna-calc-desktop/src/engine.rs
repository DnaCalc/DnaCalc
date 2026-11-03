//! Desktop composition of native return messages and CalcHost's checked P07 route.
use crate::shell::Shell;
use calc_host::{
    CalcMode, HostOptions, HostProgress, HostSession, MemorySource, OpenPolicy, ProjectedValue,
    ProjectionRequest, SheetChange, SkinIntent, WorkBudget, WorkbookCommand,
};
use calc_value::{CalcValue, CellKey};
use calc_xll_host::XllHost;
use std::{
    collections::BTreeSet,
    path::PathBuf,
    rc::Rc,
    sync::Arc,
    time::{Duration, Instant},
};

fn enter(book: &mut HostSession, at: CellKey, text: &str) -> Result<(), String> {
    enter_as(book, at, text, false)
}
fn enter_as(book: &mut HostSession, at: CellKey, text: &str, legacy: bool) -> Result<(), String> {
    let intent = if legacy {
        SkinIntent::Edit {
            edits: vec![calc_host::CellEdit::Set {
                at,
                input: calc_host::CellInput::Formula(
                    calc_host::FormulaText::with_form(
                        calc_value::ExcelText::from_utf16_code_units(
                            text.strip_prefix('=')
                                .ok_or("legacy entry needs a formula")?
                                .encode_utf16()
                                .collect(),
                        ),
                        calc_host::FormulaForm::Legacy,
                    )
                    .map_err(|e| format!("{e:?}"))?,
                ),
            }],
        }
    } else {
        SkinIntent::EnterCell {
            at,
            text: text.to_owned(),
        }
    };
    let receipt = book.dispatch(intent);
    if receipt.accepted() {
        Ok(())
    } else {
        Err(format!("cell entry refused: {:?}", receipt.error))
    }
}
fn retire_rtd(
    book: &mut HostSession,
    rtd: &calc_com_host::RtdClient,
    providers: &BTreeSet<calc_xll_host::RtdProvider>,
    at: CellKey,
    retired: &mut Vec<serde_json::Value>,
) -> Result<(), String> {
    for (provider, subscription) in rtd.disconnect_caller(providers, at)? {
        let disposition = book.retire_rtd(provider, subscription);
        if disposition != calc_host::RtdDisposition::Accepted {
            return Err(format!("RTD disconnect was not admitted: {disposition:?}"));
        }
        retired.push(serde_json::json!({"provider":format!("{provider:?}"),"subscription":subscription,"disposition":format!("{disposition:?}")}));
    }
    Ok(())
}
fn value(book: &HostSession, at: CellKey) -> Result<(CalcValue, serde_json::Value), String> {
    let page = calc_host::project(book.host(), &ProjectionRequest { cells: vec![at] })
        .map_err(|e| e.to_string())?;
    match page.cells()[0].value() {
        ProjectedValue::Calculated { value, calculation } => {
            let stamp = calculation
                .stamp()
                .ok_or("calculation has no producing stamp")?;
            Ok((
                value.clone(),
                serde_json::json!({"stamp":format!("{stamp:?}"),
                "invocations":stamp.reads.invocations.len(),
                "cooperative_invocations":stamp.reads.invocations.iter().filter(|i|!i.non_preemptible).count(),
                "completed_invocations":stamp.reads.invocations.iter().filter(|i|i.completed).count(),
                "rtd_inputs":stamp.reads.external_inputs.iter().map(|read|serde_json::json!({"provider":format!("{:?}",read.key.callable),"subscription":read.key.subscription,"sequence":read.sequence})).collect::<Vec<_>>(),
                "pass_reason":format!("{:?}",stamp.certification.pass.reason)}),
            ))
        }
        other => Err(format!("cell has no current engine calculation: {other:?}")),
    }
}
pub fn run(
    shell: &Shell,
    native: &Rc<XllHost>,
    sheet_name: &str,
    cases: Option<PathBuf>,
    formula: String,
) -> Result<serde_json::Value, String> {
    let mut book = HostSession::open(
        Arc::new(MemorySource::new(calc_host::blank_package())),
        &OpenPolicy::default(),
        HostOptions::default(),
    )
    .map_err(|e| format!("{e:?}"))?;
    book.dispatch(SkinIntent::SetCalcMode(CalcMode::Manual));
    let sheet = book.host().sheets()[0].id();
    let rename = book.dispatch(SkinIntent::Command(WorkbookCommand::Sheet {
        sheet: Some(sheet),
        change: SheetChange::Rename {
            name: sheet_name.into(),
        },
    }));
    if !rename.accepted() {
        return Err(format!("sheet rename: {:?}", rename.error));
    }
    native.set_sheet(sheet, format!("[Book1]{sheet_name}"), true);
    shell.projection()?.set_sheet(sheet_name)?;
    native.set_async_continuations(true)?;
    let rtd = Rc::new(calc_com_host::RtdClient::new()?);
    native.set_rtd_service(rtd.clone())?;
    book.set_user_functions(native.functions()?);
    let bindings = book.host().user_function_bindings();
    let providers = native
        .registrations()
        .iter()
        .filter_map(|r| bindings.bind(None, &r.name))
        .collect::<BTreeSet<_>>();
    book.set_calculation_budget(WorkBudget::evaluations(1));
    let a = CellKey::new(sheet, 1, 1).unwrap();
    let b = CellKey::new(sheet, 3, 2).unwrap();
    let c = CellKey::new(sheet, 3, 3).unwrap();
    enter(&mut book, a, "7")?;
    let cases: Vec<serde_json::Value> = match cases {
        Some(path) => serde_json::from_slice(&std::fs::read(path).map_err(|e| e.to_string())?)
            .map_err(|e| e.to_string())?,
        None => vec![serde_json::json!({"formula":formula,"value":null})],
    };
    let mut results = Vec::new();
    let mut commands = Vec::new();
    let mut returns = Vec::new();
    let mut revoked = Vec::new();
    let mut rtd_updates = Vec::new();
    let mut rtd_retired = Vec::new();
    for case in cases {
        let formula = case["formula"].as_str().ok_or("case needs formula")?;
        let legacy = case["legacy"].as_bool().unwrap_or(false);
        if let Some(dependent) = case["dependent"].as_str() {
            enter_as(&mut book, c, dependent, legacy)?;
            retire_rtd(&mut book, &rtd, &providers, c, &mut rtd_retired)?;
        }
        enter_as(&mut book, b, formula, legacy)?;
        retire_rtd(&mut book, &rtd, &providers, b, &mut rtd_retired)?;
        // Separate workbook commands retain separate read views and activations.
        // This lets the owner interleave them without moving either evaluator.
        let mut concurrent = if let Some(formula) = case["concurrent_formula"].as_str() {
            let mut peer = HostSession::open(
                Arc::new(MemorySource::new(calc_host::blank_package())),
                &OpenPolicy::default(),
                HostOptions::default(),
            )
            .map_err(|e| format!("{e:?}"))?;
            peer.dispatch(SkinIntent::SetCalcMode(CalcMode::Manual));
            peer.set_user_functions(native.functions()?);
            peer.set_calculation_budget(WorkBudget::evaluations(1));
            let cell = CellKey::new(peer.host().sheets()[0].id(), 3, 2).unwrap();
            enter(&mut peer, cell, formula)?;
            peer.dispatch(SkinIntent::Recalculate);
            Some((peer, cell))
        } else {
            None
        };
        shell.projection()?.sheet_change(b.row(), b.column())?;
        if let Some(command) = case["before_calculate"].as_str() {
            let result = native.run(command, &[])?;
            let actual = crate::value_json(&result);
            if actual.as_f64() != case["before_calculate_value"].as_f64() {
                return Err("precalculation command differs".into());
            }
            commands.push(serde_json::json!({"command":command,"value":actual}));
        }
        book.dispatch(SkinIntent::Recalculate);
        let deadline = Instant::now() + Duration::from_secs(20);
        let mut replaced = false;
        loop {
            for input in rtd.poll()? {
                let disposition = book.update_rtd(
                    input.provider,
                    input.subscription,
                    input.sequence,
                    input.value.clone(),
                );
                if disposition != calc_host::RtdDisposition::Accepted {
                    return Err(format!("RTD input was not admitted: {disposition:?}"));
                }
                rtd_updates.push(serde_json::json!({"provider":format!("{:?}",input.provider),"subscription":input.subscription,"sequence":input.sequence,"value":crate::value_json(&input.value),"disposition":format!("{disposition:?}")}));
                // This controller explicitly requests a new manual command.
                // The notification itself never completes an activation.
                book.dispatch(SkinIntent::Recalculate);
            }
            // The complete token, including workbook/target/provider generations,
            // comes from CalcHost. A native request number alone never publishes.
            for returned in native.take_async_returns() {
                let token = book
                    .host()
                    .pending_async()
                    .into_iter()
                    .find(|token| token.request == returned.request)
                    .ok_or("native return has no current P07 token")?;
                let disposition = book.complete_async(&token, returned.value?);
                if disposition != calc_host::AsyncDisposition::Accepted {
                    return Err(format!("native return rejected: {disposition:?}"));
                }
                returns.push(serde_json::json!({"request":returned.request,"token":format!("{token:?}"),"disposition":format!("{disposition:?}")}));
            }
            if book.has_pending_calculation() {
                let progress = book.advance_calculation();
                if let HostProgress::Failed(why) = progress {
                    return Err(why.to_string());
                }
            }
            if let Some((peer, _)) = &mut concurrent {
                if peer.has_pending_calculation()
                    && let HostProgress::Failed(why) = peer.advance_calculation()
                {
                    return Err(why.to_string());
                }
            }
            if !replaced
                && let Some(replacement) = case["replace_pending"].as_str()
                && let Some(token) = book.host().pending_async().first().cloned()
            {
                enter(&mut book, b, replacement)?;
                retire_rtd(&mut book, &rtd, &providers, b, &mut rtd_retired)?;
                book.dispatch(SkinIntent::Recalculate);
                let disposition = book.complete_async(&token, CalcValue::number(-123.));
                if disposition
                    != calc_host::AsyncDisposition::Rejected(calc_host::AsyncRejection::Revoked)
                {
                    return Err(format!(
                        "overwritten native token was not revoked: {disposition:?}"
                    ));
                }
                revoked.push(serde_json::json!({"token":format!("{token:?}"),"disposition":format!("{disposition:?}")}));
                replaced = true;
            }
            if !book.has_pending_calculation()
                && book.host().pending_async().is_empty()
                && native.outstanding_native_calls() == 0
                && case.get("rtd_wait_value").is_none_or(|expected| {
                    value(&book, b).is_ok_and(|(actual, _)| {
                        let actual = crate::value_json(&actual);
                        actual == *expected
                            || actual.is_number()
                                && expected.is_number()
                                && actual.as_f64() == expected.as_f64()
                    })
                })
                && concurrent.as_ref().is_none_or(|(peer, _)| {
                    !peer.has_pending_calculation() && peer.host().pending_async().is_empty()
                })
            {
                break;
            }
            if Instant::now() > deadline {
                return Err("engine native completion deadline expired".into());
            }
            shell.pump_for(Duration::from_millis(2));
        }
        let (calculated, proof) = value(&book, b)?;
        if case.get("replace_pending").is_some() && !replaced {
            return Err("native call completed before overwrite was exercised".into());
        }
        let actual = crate::value_json(&calculated);
        if !case["value"].is_null()
            && actual != case["value"]
            && !(actual.as_f64().is_some() && actual.as_f64() == case["value"].as_f64())
        {
            return Err(format!(
                "{formula}: calculated {actual}, expected {}",
                case["value"]
            ));
        }
        let (dependent, dependent_provenance) = if case.get("dependent").is_some() {
            let (calculated, proof) = value(&book, c)?;
            let actual = crate::value_json(&calculated);
            if !case["dependent_value"].is_null()
                && actual != case["dependent_value"]
                && !(actual.as_f64().is_some()
                    && actual.as_f64() == case["dependent_value"].as_f64())
            {
                return Err("dependent value differs".into());
            }
            (Some(actual), Some(proof))
        } else {
            (None, None)
        };
        let concurrent = concurrent
            .as_ref()
            .map(|(peer, cell)| {
                let (calculated, proof) = value(peer, *cell)?;
                let actual = crate::value_json(&calculated);
                if actual != case["concurrent_value"]
                    && !(actual.as_f64().is_some()
                        && actual.as_f64() == case["concurrent_value"].as_f64())
                {
                    return Err("concurrent workbook value differs".to_string());
                }
                Ok(serde_json::json!({"value":actual,"provenance":proof}))
            })
            .transpose()?;
        results.push(serde_json::json!({"formula":formula,"value":actual,"dependent":dependent,"dependent_provenance":dependent_provenance,"concurrent":concurrent,"provenance":proof}));
        shell.projection()?.after_calculate()?;
    }
    if !native.diagnostics().is_empty() {
        return Err(format!("native refusals: {:?}", native.diagnostics()));
    }
    let retained = native.functions()?;
    let stale_name = native
        .registrations()
        .iter()
        .find(|r| r.macro_type == 1)
        .ok_or("no worksheet entry")?
        .name
        .clone();
    native.close()?;
    if !native.diagnostics().is_empty() || native.live_callback_allocations() != 0 {
        return Err(format!(
            "native teardown refused: {:?}; live allocations {}",
            native.diagnostics(),
            native.live_callback_allocations()
        ));
    }
    if retained.call(&stale_name, &[])? != CalcValue::error(calc_value::WorksheetErrorCode::Name) {
        return Err("removed native entry still executes".into());
    }
    Ok(
        serde_json::json!({"host":"DNA Calc / CalcHost P07","registrations":native.registrations().len(),"results":results,"async_returns":returns,"revoked_tokens":revoked,"rtd_updates":rtd_updates,"rtd_retired":rtd_retired,"rtd_subscriptions_after_close":rtd.subscriptions(),"status_message":native.status_message(),
        "reload_results":[],"commands":commands,"live_callback_allocations":native.live_callback_allocations(),"closed_registration":"#NAME?","closed_name":stale_name}),
    )
}
