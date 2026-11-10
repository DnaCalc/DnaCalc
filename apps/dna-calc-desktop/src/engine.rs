//! Desktop composition of native return messages and CalcHost's checked P07 route.
use calc_addin_host::{AddinHost, value_json};
use calc_host::{
    CalcMode, HostOptions, HostProgress, HostSession, MemorySource, OpenPolicy, ProjectedValue,
    ProjectionRequest, SheetChange, SkinIntent, WorkBudget, WorkbookCommand,
};
use calc_platform_desktop::Shell;
use calc_value::{CalcValue, CellKey};
use std::{
    collections::BTreeSet,
    path::PathBuf,
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
    if !receipt.accepted() {
        return Err(format!("cell entry refused: {:?}", receipt.error));
    }

    Ok(())
}
fn evaluate_entry(book: &mut HostSession, at: CellKey) -> Result<(), String> {
    // Formula assignment is an evaluation of this cell on its owner even
    // in Manual. The later Calculate remains a separate native invocation.
    let entry = book.dispatch(SkinIntent::Calculate {
        scope: calc_host::CalcScope::Range(vec![calc_value::GridArea::cell(at)]),
        selection: calc_host::EvalSelection::FormulaEntry,
    });
    if !entry.accepted() {
        return Err(format!(
            "formula-entry evaluation refused: {:?}",
            entry.error
        ));
    }
    while book.has_pending_calculation() && book.host().pending_async().is_empty() {
        if let HostProgress::Failed(why) = book.advance_calculation() {
            return Err(why.to_string());
        }
    }
    Ok(())
}
// All native publications, including formula entry, use this one P07 token route.
fn deliver_native_returns(
    addins: &AddinHost,
    book: &mut HostSession,
    returns: &mut Vec<serde_json::Value>,
) -> Result<(), String> {
    for returned in addins.take_async_returns() {
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
    Ok(())
}
fn settle_entry(
    shell: &Shell,
    addins: &AddinHost,
    book: &mut HostSession,
    returns: &mut Vec<serde_json::Value>,
) -> Result<(), String> {
    let deadline = Instant::now() + Duration::from_secs(20);
    while book.has_pending_calculation() || !book.host().pending_async().is_empty() {
        deliver_native_returns(addins, book, returns)?;
        if book.has_pending_calculation() {
            if let HostProgress::Failed(why) = book.advance_calculation() {
                return Err(why.to_string());
            }
        }
        if Instant::now() > deadline {
            return Err("native formula-entry completion deadline expired".into());
        }
        shell.pump_for(Duration::from_millis(2));
    }
    Ok(())
}
/// Move the RTD values the host admitted into the evidence list, in admission order: those this
/// controller delivered from the client, and those the host admitted itself for a connection made
/// during evaluation (the value ConnectData returned, admitted as sequence 1 before publication).
fn record_admissions(book: &mut HostSession, out: &mut Vec<serde_json::Value>) {
    for admission in book.take_rtd_admissions() {
        out.push(serde_json::json!({"topic":format!("{:?}",admission.topic),"lease":admission.lease.0,"sequence":admission.sequence,"value":value_json(&admission.value),"disposition":"Accepted"}));
    }
}
fn retire_rtd(
    book: &mut HostSession,
    rtd: &calc_addin_host::RtdClient,
    providers: &BTreeSet<calc_xll_host::RtdProvider>,
    at: CellKey,
    retired: &mut Vec<serde_json::Value>,
) -> Result<(), String> {
    for (topic, lease) in rtd.begin_replacement(providers, at)? {
        let disposition = book.retire_rtd(topic.clone(), lease);
        if disposition != calc_host::RtdDisposition::Accepted {
            return Err(format!("RTD disconnect was not admitted: {disposition:?}"));
        }
        retired.push(serde_json::json!({"topic":format!("{topic:?}"),"lease":lease.0,"disposition":format!("{disposition:?}")}));
    }
    Ok(())
}
fn reconcile_rtd(
    book: &HostSession,
    rtd: &calc_addin_host::RtdClient,
    providers: &BTreeSet<calc_xll_host::RtdProvider>,
    cells: &[CellKey],
) -> Result<(), String> {
    let page = calc_host::project(
        book.host(),
        &ProjectionRequest {
            cells: cells.to_vec(),
        },
    )
    .map_err(|e| e.to_string())?;
    for cell in page.cells() {
        rtd.finish_replacement(providers, cell.at())?;
        if let ProjectedValue::Calculated { calculation, .. } = cell.value() {
            let stamp = calculation
                .stamp()
                .ok_or("RTD reconciliation needs a producing stamp")?;
            rtd.reconcile_evaluation(
                providers,
                cell.at(),
                &stamp
                    .reads
                    .invocations
                    .iter()
                    .map(|call| call.runtime)
                    .collect(),
            )?;
        }
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
                "rtd_inputs":stamp.reads.external_inputs.iter().map(|read|serde_json::json!({"topic":format!("{:?}",read.topic),"lease":read.lease.0,"sequence":read.sequence})).collect::<Vec<_>>(),
                "pass_reason":format!("{:?}",stamp.certification.pass.reason)}),
            ))
        }
        other => Err(format!("cell has no current engine calculation: {other:?}")),
    }
}
pub fn run(
    shell: &Shell,
    addins: &mut AddinHost,
    sheet_name: &str,
    workbook_name: &str,
    cases: Option<PathBuf>,
    formula: String,
    early_execution: &[u64],
) -> Result<serde_json::Value, String> {
    let mut book = HostSession::open(
        Arc::new(MemorySource::new(calc_host::blank_package())),
        &OpenPolicy::default(),
        HostOptions::default(),
    )
    .map_err(|e| format!("{e:?}"))?;
    book.set_file_identity(workbook_name, false);
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
    addins.bind_sheet(sheet, sheet_name);
    addins.set_active_sheet(sheet_name)?;
    addins.enable_async_continuations()?;
    let rtd = addins.enable_rtd()?;
    book.set_rtd_connector(rtd.clone());
    book.set_user_functions(addins.callables_with_early_execution(early_execution)?);
    let bindings = book.host().user_function_bindings();
    let providers = addins
        .registrations()
        .iter()
        .filter_map(|r| bindings.bind(None, &r.name))
        .collect::<BTreeSet<_>>();
    book.set_calculation_budget(WorkBudget::evaluations(1));
    let a = CellKey::new(sheet, 1, 1).unwrap();
    let b = CellKey::new(sheet, 3, 2).unwrap();
    enter(&mut book, a, "7")?;
    let cases: Vec<serde_json::Value> = match cases {
        Some(path) => serde_json::from_slice(&std::fs::read(path).map_err(|e| e.to_string())?)
            .map_err(|e| e.to_string())?,
        None => vec![serde_json::json!({"formula":formula,"value":null})],
    };
    if workbook_name != "Book1"
        && cases
            .iter()
            .any(|case| case.get("concurrent_formula").is_some())
    {
        return Err("dc-snz.23: named concurrent workbook contexts are not modelled".into());
    }
    let dependent_address = cases
        .iter()
        .find_map(|case| case["dependent_cell"].as_str())
        .unwrap_or("C3")
        .to_owned();
    if cases.iter().any(|case| {
        case.get("dependent").is_some()
            && case["dependent_cell"].as_str().unwrap_or("C3") != dependent_address
    }) {
        return Err("one case profile needs one dependent cell".into());
    }
    let calc_host::ReferenceTarget::Area(dependent_area) =
        book.host().resolve_reference_text(b, &dependent_address)
    else {
        return Err("dependent needs a local cell".into());
    };
    if dependent_area.cell_count() != 1 || dependent_area.sheet() != sheet {
        return Err("dependent needs one local cell".into());
    }
    let c = dependent_area.first();
    let mut results = Vec::new();
    let mut commands = Vec::new();
    let mut returns = Vec::new();
    let mut revoked = Vec::new();
    let mut rtd_updates = Vec::new();
    let mut rtd_retired = Vec::new();
    let mut formula_entries = Vec::new();
    let mut current_formula: Option<String> = None;
    for case in cases {
        let formula = case["formula"].as_str().ok_or("case needs formula")?;
        let legacy = case["legacy"].as_bool().unwrap_or(false);
        let entry_evaluation = case["formula_entry"].as_bool().unwrap_or(legacy);
        if let Some(dependent) = case["dependent"].as_str() {
            enter_as(&mut book, c, dependent, legacy)?;
            retire_rtd(&mut book, &rtd, &providers, c, &mut rtd_retired)?;
            if entry_evaluation {
                evaluate_entry(&mut book, c)?;
                settle_entry(shell, addins, &mut book, &mut returns)?;
                reconcile_rtd(&book, &rtd, &providers, &[c])?;
                let (calculated, proof) = value(&book, c)?;
                formula_entries.push(serde_json::json!({"formula":dependent,"cell":dependent_address,"value":value_json(&calculated),"provenance":proof}));
            }
        }
        if let Some(edit) = case.get("edit") {
            let address = edit["cell"].as_str().ok_or("edit needs cell")?;
            let calc_host::ReferenceTarget::Area(area) =
                book.host().resolve_reference_text(b, address)
            else {
                return Err("edit needs a local cell reference".into());
            };
            if area.cell_count() != 1 || area.sheet() != sheet {
                return Err("edit needs one local cell".into());
            }
            let number = edit["value"].as_f64().ok_or("edit needs a number")?;
            let receipt = book.dispatch(SkinIntent::EnterCell {
                at: area.first(),
                text: number.to_string(),
            });
            if !receipt.accepted() {
                return Err(format!("input edit refused: {:?}", receipt.error));
            }
            addins.sheet_changed(area.first().row(), area.first().column())?;
        }
        let keep_formula = case["keep_formula"].as_bool().unwrap_or(false);
        if keep_formula && current_formula.as_deref() != Some(formula) {
            return Err("retained formula differs from selected case".into());
        }
        if !keep_formula {
            current_formula = Some(formula.to_owned());
            // A case formula's {pid} is this process's id, as the owned Excel's is its own.
            enter_as(
                &mut book,
                b,
                &formula.replace("{pid}", &std::process::id().to_string()),
                legacy,
            )?;
            retire_rtd(&mut book, &rtd, &providers, b, &mut rtd_retired)?;
            if entry_evaluation {
                evaluate_entry(&mut book, b)?;
                settle_entry(shell, addins, &mut book, &mut returns)?;
                reconcile_rtd(&book, &rtd, &providers, &[b])?;
                let (calculated, proof) = value(&book, b)?;
                formula_entries.push(serde_json::json!({"formula":formula,"cell":"B3","value":value_json(&calculated),"provenance":proof}));
            }
        }
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
            peer.set_user_functions(addins.callables_with_early_execution(early_execution)?);
            peer.set_calculation_budget(WorkBudget::evaluations(1));
            let cell = CellKey::new(peer.host().sheets()[0].id(), 3, 2).unwrap();
            enter(&mut peer, cell, formula)?;
            peer.dispatch(SkinIntent::Recalculate);
            Some((peer, cell))
        } else {
            None
        };
        if !keep_formula {
            addins.sheet_changed(b.row(), b.column())?;
        }
        if let Some(command) = case["before_calculate"].as_str() {
            let result = addins.run(command, &[])?;
            let actual = value_json(&result);
            if actual.as_f64() != case["before_calculate_value"].as_f64() {
                return Err("precalculation command differs".into());
            }
            commands.push(serde_json::json!({"command":command,"value":actual}));
        }
        let calculation = case["calculation"].as_str().unwrap_or("dirty");
        // `range` ("B3,C3") calculates those cells in the listed order, as run.ps1's
        // Range.Calculate does in Excel.
        let range = match case["range"].as_str() {
            Some(text) => {
                let mut areas = Vec::new();
                for address in text.split(',') {
                    let calc_host::ReferenceTarget::Area(area) =
                        book.host().resolve_reference_text(b, address.trim())
                    else {
                        return Err("range needs local cell references".into());
                    };
                    areas.push(area);
                }
                Some(areas)
            }
            None => None,
        };
        let admission = book.dispatch(match calculation {
            _ if range.is_some() => SkinIntent::CalculateRange {
                areas: range.clone().unwrap(),
            },
            "dirty" => SkinIntent::Recalculate,
            "full" => SkinIntent::Calculate {
                scope: calc_host::CalcScope::Workbook,
                selection: calc_host::EvalSelection::ForceAll,
            },
            "rebuild" => SkinIntent::Rebuild,
            _ => return Err("unknown engine calculation command".into()),
        });
        if !admission.accepted() {
            return Err(format!(
                "{calculation} calculation refused: {:?}",
                admission.error
            ));
        }
        let deadline = Instant::now() + Duration::from_secs(20);
        let idle = Duration::from_millis(case["rtd_idle_ms"].as_u64().unwrap_or(0));
        let mut idle_until = None;
        let mut replaced = false;
        loop {
            let evaluating = book.has_pending_calculation()
                || !book.host().pending_async().is_empty()
                || addins.native().outstanding_native_calls() != 0
                || concurrent.as_ref().is_some_and(|(peer, _)| {
                    peer.has_pending_calculation() || !peer.host().pending_async().is_empty()
                });
            record_admissions(&mut book, &mut rtd_updates);
            let inputs = addins.poll_rtd(evaluating)?;
            for input in inputs {
                let disposition = book.update_rtd(
                    input.topic.clone(),
                    input.lease,
                    input.sequence,
                    input.value.clone(),
                );
                match disposition {
                    // The host already admitted the connection's own value as it was
                    // returned; the client's queued copy of it is the same admission.
                    calc_host::RtdDisposition::Duplicate => continue,
                    calc_host::RtdDisposition::Accepted => {}
                    other => return Err(format!("RTD input was not admitted: {other:?}")),
                }
                record_admissions(&mut book, &mut rtd_updates);
                // This controller explicitly requests a new manual command.
                // The notification itself never completes an activation.
                book.dispatch(SkinIntent::Recalculate);
            }
            deliver_native_returns(addins, &mut book, &mut returns)?;
            if book.has_pending_calculation() {
                let progress = book.advance_calculation();
                if let HostProgress::Failed(why) = progress {
                    return Err(why.to_string());
                }
                record_admissions(&mut book, &mut rtd_updates);
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
                && addins.native().outstanding_native_calls() == 0
                && !rtd.has_pending_inputs()
            {
                reconcile_rtd(&book, &rtd, &providers, &[b, c])?;
            }
            let complete = !book.has_pending_calculation()
                && book.host().pending_async().is_empty()
                && addins.native().outstanding_native_calls() == 0
                && !rtd.has_pending_inputs()
                && case.get("rtd_wait_value").is_none_or(|expected| {
                    value(&book, b).is_ok_and(|(actual, _)| {
                        let actual = value_json(&actual);
                        actual == *expected
                            || actual.is_number()
                                && expected.is_number()
                                && actual.as_f64() == expected.as_f64()
                    })
                })
                && concurrent.as_ref().is_none_or(|(peer, _)| {
                    !peer.has_pending_calculation() && peer.host().pending_async().is_empty()
                });
            if complete && idle_until.is_none() {
                if idle.is_zero() {
                    break;
                }
                idle_until = Some(
                    Instant::now()
                        .checked_add(idle)
                        .ok_or("RTD idle interval overflow")?,
                );
            }
            if let Some(until) = idle_until {
                if Instant::now() >= until {
                    if complete {
                        break;
                    }
                    return Err("RTD idle observation ended with incomplete calculation".into());
                }
            } else if Instant::now() > deadline {
                return Err(format!(
                    "engine native completion deadline expired (calculation pending {}, async {}, native calls {}, rtd inputs {})",
                    book.has_pending_calculation(),
                    book.host().pending_async().len(),
                    addins.native().outstanding_native_calls(),
                    rtd.has_pending_inputs()
                ));
            }
            shell.pump_for(Duration::from_millis(2));
        }
        let idle_observed_ms =
            idle_until.map(|until| (Instant::now() - (until - idle)).as_millis());
        let (calculated, proof) = value(&book, b)?;
        if case.get("replace_pending").is_some() && !replaced {
            return Err("native call completed before overwrite was exercised".into());
        }
        let actual = value_json(&calculated);
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
            let actual = value_json(&calculated);
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
                let actual = value_json(&calculated);
                if actual != case["concurrent_value"]
                    && !(actual.as_f64().is_some()
                        && actual.as_f64() == case["concurrent_value"].as_f64())
                {
                    return Err("concurrent workbook value differs".to_string());
                }
                Ok(serde_json::json!({"value":actual,"provenance":proof}))
            })
            .transpose()?;
        let mut spill_cells = serde_json::Map::new();
        if let Some(cells) = case["spill_cells"].as_object() {
            for (address, expected) in cells {
                let calc_host::ReferenceTarget::Area(area) =
                    book.host().resolve_reference_text(b, address)
                else {
                    return Err("spill probe needs a cell".into());
                };
                if area.cell_count() != 1 || area.sheet() != sheet {
                    return Err("spill probe needs one local cell".into());
                }
                let (actual, proof) = value(&book, area.first())?;
                let actual = value_json(&actual);
                if actual != *expected
                    && !(actual.is_number()
                        && expected.is_number()
                        && actual.as_f64() == expected.as_f64())
                {
                    return Err(format!(
                        "spill {address}: calculated {actual}, expected {expected}"
                    ));
                }
                spill_cells.insert(
                    address.clone(),
                    serde_json::json!({"value":actual,"provenance":proof}),
                );
            }
        }
        results.push(serde_json::json!({"formula":formula,"calculation":calculation,"value":actual,"dependent":dependent,"dependent_provenance":dependent_provenance,"concurrent":concurrent,"provenance":proof,"idle_observed_ms":idle_observed_ms,"rtd_updates_through":rtd_updates.len(),"spill_cells":spill_cells}));
        addins.after_calculate()?;
    }
    let native = addins.native();
    if !native.diagnostics().is_empty() {
        return Err(format!("native refusals: {:?}", native.diagnostics()));
    }
    let retained = addins.callables_with_early_execution(early_execution)?;
    let stale_name = addins
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
        serde_json::json!({"host":"DNA Calc / CalcHost P07","registrations":addins.registrations().len(),"early_execution_registrations":early_execution,"results":results,"formula_entries":formula_entries,"async_returns":returns,"revoked_tokens":revoked,"rtd_updates":rtd_updates,"rtd_retired":rtd_retired,"rtd_subscriptions_after_close":rtd.subscriptions(),"status_message":native.status_message(),
        "reload_results":[],"commands":commands,"live_callback_allocations":native.live_callback_allocations(),"closed_registration":"#NAME?","closed_name":stale_name}),
    )
}
