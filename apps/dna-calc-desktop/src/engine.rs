//! Desktop composition of native return messages and CalcHost's checked P07 route.
use crate::shell::Shell;
use calc_host::{
    CalcMode, HostOptions, HostProgress, HostSession, MemorySource, OpenPolicy, ProjectedValue,
    ProjectionRequest, SheetChange, SkinIntent, WorkBudget, WorkbookCommand,
};
use calc_value::{CalcValue, CellKey};
use calc_xll_host::XllHost;
use std::{
    path::PathBuf,
    rc::Rc,
    sync::Arc,
    time::{Duration, Instant},
};

fn enter(book: &mut HostSession, at: CellKey, text: &str) -> Result<(), String> {
    let receipt = book.dispatch(SkinIntent::EnterCell {
        at,
        text: text.to_owned(),
    });
    if receipt.accepted() {
        Ok(())
    } else {
        Err(format!("cell entry refused: {:?}", receipt.error))
    }
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
    book.set_user_functions(native.functions()?);
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
    let mut returns = Vec::new();
    for case in cases {
        let formula = case["formula"].as_str().ok_or("case needs formula")?;
        if let Some(dependent) = case["dependent"].as_str() {
            enter(&mut book, c, dependent)?;
        }
        enter(&mut book, b, formula)?;
        shell.projection()?.sheet_change(b.row(), b.column())?;
        book.dispatch(SkinIntent::Recalculate);
        let deadline = Instant::now() + Duration::from_secs(20);
        loop {
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
            if !book.has_pending_calculation() && book.host().pending_async().is_empty() {
                break;
            }
            if Instant::now() > deadline {
                return Err("engine native completion deadline expired".into());
            }
            shell.pump_for(Duration::from_millis(2));
        }
        let (calculated, proof) = value(&book, b)?;
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
        let dependent = if case.get("dependent").is_some() {
            let (calculated, _) = value(&book, c)?;
            let actual = crate::value_json(&calculated);
            if !case["dependent_value"].is_null()
                && actual != case["dependent_value"]
                && !(actual.as_f64().is_some()
                    && actual.as_f64() == case["dependent_value"].as_f64())
            {
                return Err("dependent value differs".into());
            }
            Some(actual)
        } else {
            None
        };
        results.push(serde_json::json!({"formula":formula,"value":actual,"dependent":dependent,"provenance":proof}));
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
        serde_json::json!({"host":"DNA Calc / CalcHost P07","registrations":native.registrations().len(),"results":results,"async_returns":returns,
        "reload_results":[],"commands":[],"live_callback_allocations":native.live_callback_allocations(),"closed_registration":"#NAME?","closed_name":stale_name}),
    )
}
