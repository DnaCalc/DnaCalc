//! dc-y2o: circular references through calc-host, with the oracle workbook calc-engine's
//! dc_y2o case captured from Excel 16.0.20527 (calc-engine's tests/fixtures/dc_y2o, made by
//! make-dc-y2o-oracles.ps1; iteration off, values stored by hand). Opened and recalculated, the
//! formulas on a circular reference, and those that read one, keep what their cells stored, marked
//! as the circular-reference rule's values, and the host lists them for a warning (dc-y2o.1). The
//! workbook's iteration settings are switched by a transaction that undo takes back, applied by the
//! next recalculation, and saved in the workbook part's `calcPr` (dc-y2o.2).

// Case names are fixed identities of the form `<bead>__<case>`.
#![allow(non_snake_case)]

use calc_host::value::{CalcValue, CellKey, SheetId};
use calc_host::{
    CalcMode, CellEdit, HostOptions, HostSession, HostState, IterationSettings, MemorySource,
    OpenPolicy, PackageRead, ProjectedValue, ProjectionRequest, SavePolicy, SkinIntent, project,
    save,
};
use std::sync::Arc;

fn oracle() -> Vec<u8> {
    include_bytes!("../../calc-engine/tests/fixtures/dc_y2o/oracle.xlsx").to_vec()
}

fn session(bytes: &[u8]) -> HostSession {
    let source: Arc<dyn PackageRead> = Arc::new(MemorySource::new(bytes.to_vec()));
    let mut session = HostSession::open(
        source,
        &OpenPolicy::default(),
        HostOptions {
            calc_mode: Some(CalcMode::Automatic),
            ..HostOptions::default()
        },
    )
    .expect("it opens");
    let receipt = session.dispatch(SkinIntent::Recalculate);
    assert!(receipt.accepted(), "{receipt:?}");
    session
}

fn off_sheet(host: &HostState) -> SheetId {
    host.sheets()
        .iter()
        .find(|sheet| sheet.name().text().to_string_lossy() == "Off")
        .expect("sheet Off")
        .id()
}

/// The cell's value, and whether the circular-reference rule held it.
fn value(host: &HostState, row: u32, column: u32) -> (CalcValue, bool) {
    let at = CellKey::new(off_sheet(host), row, column).unwrap();
    match project(host, &ProjectionRequest { cells: vec![at] })
        .unwrap()
        .cells()[0]
        .value()
    {
        ProjectedValue::Calculated { value, calculation } => {
            (value.clone(), calculation.held_by_circular_rule())
        }
        other => panic!("{at:?}: {other:?}"),
    }
}

fn set_iteration(session: &mut HostSession, settings: IterationSettings) {
    let receipt = session.dispatch(SkinIntent::Edit {
        edits: vec![CellEdit::SetIteration(settings)],
    });
    assert!(receipt.accepted(), "{receipt:?}");
}

fn workbook_part(package: &[u8]) -> String {
    let source: Arc<dyn PackageRead> = Arc::new(MemorySource::new(package.to_vec()));
    let document = calc_doc::open(source, &OpenPolicy::default()).expect("the saved package opens");
    String::from_utf8(
        document
            .package()
            .read_part("xl/workbook.xml")
            .expect("the part"),
    )
    .unwrap()
}

/// Opened with iteration off, Off!A1 `=B1+1` and B1 `=A1+1` keep 77 and 55, G1 `=A1*10` 999, as
/// Excel showed them after a full rebuild; H1 `=C1*100` is calculated (100). The host lists the
/// cells on a circular reference.
#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn dc_y2o__the_oracle_holds_its_stored_values_by_the_circular_rule() {
    let session = session(&oracle());
    let host = session.host();
    let n = CalcValue::number;
    assert_eq!(value(host, 1, 1), (n(77.0), true));
    assert_eq!(value(host, 1, 2), (n(55.0), true));
    assert_eq!(value(host, 1, 7), (n(999.0), true));
    assert_eq!(value(host, 1, 8), (n(100.0), false));
    let sheet = off_sheet(host);
    let circular = host.circular_cells();
    assert!(circular.contains(&CellKey::new(sheet, 1, 1).unwrap()));
    assert!(!circular.contains(&CellKey::new(sheet, 1, 8).unwrap()));
    assert!(!host.recalc_context().iteration);
}

/// Iteration switched on (at most 1 iteration) calculates the cycle from the values its cells
/// held, in cell order: A1 = 55 + 1, then B1 = 56 + 1, and G1 reads A1 (560). Undo switches it off
/// again: the cycle keeps what the iteration left. Saved with iteration on, the workbook part's
/// `calcPr` holds `iterate="1"` and `iterateCount="1"` (the maximum change, Excel's default, left
/// out), and the saved package opens with iteration on; saved after the undo, it is written as the
/// file had it.
#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn dc_y2o__iteration_is_switched_undone_and_saved() {
    let mut session = session(&oracle());
    let n = CalcValue::number;
    set_iteration(&mut session, IterationSettings::new(true, 1, 0.001));
    let host = session.host();
    assert!(host.recalc_context().iteration);
    assert_eq!(host.recalc_context().max_iterations, 1);
    assert_eq!(value(host, 1, 1), (n(56.0), false));
    assert_eq!(value(host, 1, 2), (n(57.0), false));
    assert_eq!(value(host, 1, 7), (n(560.0), false));

    let mut saved = Vec::new();
    save(session.host(), &SavePolicy::default(), &mut saved).expect("saved");
    let part = workbook_part(&saved);
    assert!(part.contains(r#"iterate="1""#), "{part}");
    assert!(part.contains(r#"iterateCount="1""#), "{part}");
    assert!(!part.contains("iterateDelta"), "{part}");
    let reopened = session_without_recalc(&saved);
    assert!(reopened.host().recalc_context().iteration);
    assert_eq!(reopened.host().recalc_context().max_iterations, 1);

    let receipt = session.dispatch(SkinIntent::Undo);
    assert!(receipt.accepted(), "{receipt:?}");
    let host = session.host();
    assert!(!host.recalc_context().iteration);
    assert_eq!(value(host, 1, 1), (n(56.0), true));
    assert_eq!(value(host, 1, 2), (n(57.0), true));
    let mut saved = Vec::new();
    save(session.host(), &SavePolicy::default(), &mut saved).expect("saved");
    let part = workbook_part(&saved);
    assert!(!part.contains("iterate"), "{part}");

    // A maximum change other than Excel's default is written; iteration off drops the attributes.
    set_iteration(&mut session, IterationSettings::new(true, 100, 0.5));
    let mut saved = Vec::new();
    save(session.host(), &SavePolicy::default(), &mut saved).expect("saved");
    let part = workbook_part(&saved);
    assert!(part.contains(r#"iterate="1""#), "{part}");
    assert!(part.contains(r#"iterateDelta="0.5""#), "{part}");
    assert!(!part.contains("iterateCount"), "{part}");
    let reopened = session_without_recalc(&saved);
    assert_eq!(
        reopened.host().recalc_context().max_change_bits,
        0.5f64.to_bits()
    );
    set_iteration(&mut session, IterationSettings::new(false, 100, 0.5));
    let mut saved = Vec::new();
    save(session.host(), &SavePolicy::default(), &mut saved).expect("saved");
    let part = workbook_part(&saved);
    assert!(!part.contains("iterate="), "{part}");
    assert!(part.contains(r#"iterateDelta="0.5""#), "{part}");
}

fn session_without_recalc(bytes: &[u8]) -> HostSession {
    let source: Arc<dyn PackageRead> = Arc::new(MemorySource::new(bytes.to_vec()));
    HostSession::open(source, &OpenPolicy::default(), HostOptions::default()).expect("it opens")
}

/// Recorded first-calculation values, including held seeds and sheet/cell order, must survive
/// a yield at any position of an iterative sweep. Reopen observations are deliberately separate:
/// reopening and calculating again advances divergent cycles in the recorded Excel sessions.
#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn dc_hsa5_11__recorded_iteration_is_independent_of_owner_budget() {
    use calc_host::{CompletionOutcome, HostProgress, WorkBudget};
    let fixtures: [(&[u8], &[u8]); 2] = [
        (
            include_bytes!("../../calc-engine/tests/fixtures/dc_86f/oracle.xlsx"),
            include_bytes!("../../calc-engine/tests/captures/dc_86f/oracle.capture.json"),
        ),
        (
            include_bytes!("../../calc-engine/tests/fixtures/dc_10y_iteration/oracle.xlsx"),
            include_bytes!("../../calc-engine/tests/captures/dc_10y_iteration/oracle.capture.json"),
        ),
    ];
    for (bytes, capture) in fixtures {
        let record = calc_verdict::CaptureRecord::decode(capture).unwrap();
        assert_eq!(
            record.source,
            format!("sha256:{}", calc_verdict::sha256_hex(bytes))
        );
        for budget in [
            usize::MAX,
            1,
            record.formulas.len(),
            record.formulas.len() + 1,
        ] {
            let mut book = session_without_recalc(bytes);
            book.set_calculation_budget(WorkBudget::evaluations(budget));
            let cells: Vec<_> = record
                .formulas
                .iter()
                .map(|formula| {
                    let s = book
                        .host()
                        .sheets()
                        .iter()
                        .find(|s| s.name().text().to_string_lossy() == formula.cell.sheet())
                        .unwrap()
                        .id();
                    CellKey::new(s, formula.cell.row(), formula.cell.column()).unwrap()
                })
                .collect();
            let before = project(
                book.host(),
                &ProjectionRequest {
                    cells: cells.clone(),
                },
            )
            .unwrap();
            let receipt = book.dispatch(SkinIntent::Rebuild);
            assert!(receipt.accepted(), "{:?}", receipt.error);
            if budget == 1 {
                assert!(book.has_pending_calculation());
                book.cancel_calculation();
                assert!(!book.has_pending_calculation());
                let cancelled = book.take_calculation_events();
                assert_eq!(cancelled.iter().filter(|event| matches!(event,
                    calc_host::CalcEvent::Completed(r) if r.outcome == CompletionOutcome::Cancelled
                )).count(), 1);
                assert_eq!(
                    project(
                        book.host(),
                        &ProjectionRequest {
                            cells: cells.clone()
                        }
                    )
                    .unwrap()
                    .cells(),
                    before.cells()
                );
                // The cancelled candidate must not become the next command's held seed.
                assert!(book.dispatch(SkinIntent::Rebuild).accepted());
            }
            let mut steps = 0;
            while book.has_pending_calculation() {
                // Current DNA publication policy: no unfinished sweep becomes a displayed value.
                // P14 asks whether Excel exposes a different intermediate cached view.
                assert_eq!(
                    project(
                        book.host(),
                        &ProjectionRequest {
                            cells: cells.clone()
                        }
                    )
                    .unwrap()
                    .cells(),
                    before.cells()
                );
                let progress = book.advance_calculation();
                assert!(!matches!(progress, HostProgress::Failed(_)), "{progress:?}");
                steps += 1;
                assert!(steps < 10000, "bounded iterative work must finish");
            }
            let projected = project(book.host(), &ProjectionRequest { cells }).unwrap();
            for (formula, cell) in record.formulas.iter().zip(projected.cells()) {
                let expected = record
                    .observations
                    .iter()
                    .find(|o| o.cell == formula.cell)
                    .unwrap();
                match cell.value() {
                    ProjectedValue::Calculated { value, calculation } => {
                        assert_eq!(
                            value,
                            expected.observation.expected(),
                            "{} {:?}, budget {budget}",
                            record.case,
                            formula.cell
                        );
                        assert!(!calculation.held_by_circular_rule());
                        assert!(!calculation.reads_external_links());
                    }
                    other => panic!("{} {:?}: {other:?}", record.case, formula.cell),
                }
            }
            let terminal = book
                .take_calculation_events()
                .into_iter()
                .filter_map(|e| match e {
                    calc_host::CalcEvent::Completed(r) => Some(r),
                    _ => None,
                })
                .collect::<Vec<_>>();
            assert_eq!(terminal.len(), 1);
            assert_eq!(terminal[0].outcome, CompletionOutcome::Incomplete);
            assert_eq!(terminal[0].coverage.calculated, record.formulas.len());
            assert!(
                terminal[0].coverage.uncertified > 0,
                "iteration is not a SliceV1 freshness proof"
            );
        }
    }
}

/// DNA repair-wave policy, not a new Excel observation: a spill can expose an executed
/// INDIRECT cycle after the first SCC already converged. That new work needs a stopping decision.
#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn dc_hsa5_11__a_spill_repair_reopens_the_iteration_decision() {
    use calc_host::{HostProgress, WorkBudget};
    for budget in [usize::MAX, 1, 2, 3, 4, 7] {
        let mut book = session_without_recalc(&calc_host::blank_package());
        book.set_calculation_budget(WorkBudget::evaluations(budget));
        assert!(
            book.dispatch(SkinIntent::SetCalcMode(CalcMode::Manual))
                .accepted()
        );
        let s = book.host().sheets()[0].id();
        let at = |r, c| CellKey::new(s, r, c).unwrap();
        let mut edits = vec![CellEdit::SetIteration(IterationSettings::new(
            true, 5, 0.001,
        ))];
        for (cell, formula) in [
            (at(1, 1), "=A1*0"),
            (at(1, 3), "=IF(D2=0,0,INDIRECT(\"C1\")+1)"),
            (at(1, 4), "=SEQUENCE(2)"),
        ] {
            edits.push(CellEdit::Set {
                at: cell,
                input: calc_host::interpret_entry(formula).unwrap().unwrap(),
            });
        }
        assert!(book.dispatch(SkinIntent::Edit { edits }).accepted());
        assert!(book.dispatch(SkinIntent::Recalculate).accepted());
        for _ in 0..1000 {
            if !book.has_pending_calculation() {
                break;
            }
            let progress = book.advance_calculation();
            assert!(!matches!(progress, HostProgress::Failed(_)), "{progress:?}");
        }
        assert!(!book.has_pending_calculation());
        let page = project(
            book.host(),
            &ProjectionRequest {
                cells: vec![at(1, 1), at(1, 3), at(2, 4)],
            },
        )
        .unwrap();
        for (cell, expected) in page.cells().iter().zip([0.0, 4.0, 2.0]) {
            assert!(
                matches!(cell.value(), ProjectedValue::Calculated { value, calculation } if value.as_number() == Some(expected) && !calculation.held_by_circular_rule()),
                "budget {budget}: {cell:?}"
            );
        }
    }
}
