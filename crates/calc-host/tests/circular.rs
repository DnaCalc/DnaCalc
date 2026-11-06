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

fn p14_value(raw: &serde_json::Value) -> CalcValue {
    use calc_host::value::{ExcelText, WorksheetErrorCode};
    if let Some(bits) = raw["number"].as_str() {
        CalcValue::number(f64::from_bits(u64::from_str_radix(bits, 16).unwrap()))
    } else if let Some(value) = raw["logical"].as_bool() {
        CalcValue::logical(value)
    } else if let Some(units) = raw["text"].as_str() {
        assert_eq!(units.len() % 4, 0);
        CalcValue::text(ExcelText::from_utf16_code_units(
            (0..units.len())
                .step_by(4)
                .map(|i| u16::from_str_radix(&units[i..i + 4], 16).unwrap())
                .collect(),
        ))
    } else {
        CalcValue::error(match raw["error_code"].as_u64().unwrap() {
            2007 => WorksheetErrorCode::Div0,
            2042 => WorksheetErrorCode::NA,
            code => panic!("unhandled recorded error {code}"),
        })
    }
}

/// Eng10 fresh-process COM observations distinguish logical outputs from numeric
/// stopping samples, and an all-error result from a numeric zero change. These
/// final values do not establish general SCC execution counts or mid-cycle views.
#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn dc_hsa5_11__p14_nonnumeric_stopping() {
    use calc_host::{HostProgress, WorkBudget};
    use serde_json::Value;
    let records = std::str::from_utf8(include_bytes!(
        "../../calc-engine/tests/fixtures/excel-calc-probes/P14/eng10-20261104/stopping-observations.jsonl"
    )).unwrap().lines().map(|line|serde_json::from_str::<Value>(line).unwrap()).collect::<Vec<_>>();
    assert_eq!(records.len(), 15);
    let mut comparisons = 0;
    for record in records {
        let result = &record["result"];
        assert!(result["fatal"].is_null());
        assert_eq!(result["condition"]["operation"], "Rebuild");
        let name = record["condition"].as_str().unwrap();
        let bytes: &[u8] = match name {
            "E10-B6-boolean-n2-3fefffffffffffff" => include_bytes!(
                "../../calc-engine/tests/fixtures/excel-calc-probes/P14/eng10-20261104/inputs/E10-B6-boolean-n2-3fefffffffffffff.xlsx"
            ),
            "E10-B6-boolean-n2-3ff0000000000000" => include_bytes!(
                "../../calc-engine/tests/fixtures/excel-calc-probes/P14/eng10-20261104/inputs/E10-B6-boolean-n2-3ff0000000000000.xlsx"
            ),
            "E10-B6-boolean-n2-3ff0000000000001" => include_bytes!(
                "../../calc-engine/tests/fixtures/excel-calc-probes/P14/eng10-20261104/inputs/E10-B6-boolean-n2-3ff0000000000001.xlsx"
            ),
            "E10-B6-boolean-n2-0000000000000000" => include_bytes!(
                "../../calc-engine/tests/fixtures/excel-calc-probes/P14/eng10-20261104/inputs/E10-B6-boolean-n2-0000000000000000.xlsx"
            ),
            "E10-B6-error-n2-0000000000000000-peer-0" => include_bytes!(
                "../../calc-engine/tests/fixtures/excel-calc-probes/P14/eng10-20261104/inputs/E10-B6-error-n2-0000000000000000-peer-0.xlsx"
            ),
            "E10-B6-error-n2-0000000000000000-peer-1" => include_bytes!(
                "../../calc-engine/tests/fixtures/excel-calc-probes/P14/eng10-20261104/inputs/E10-B6-error-n2-0000000000000000-peer-1.xlsx"
            ),
            "E10-B6-boolean-n5-3fefffffffffffff" => include_bytes!(
                "../../calc-engine/tests/fixtures/excel-calc-probes/P14/eng10-20261104/inputs/E10-B6-boolean-n5-3fefffffffffffff.xlsx"
            ),
            "E10-B6-boolean-n5-3ff0000000000000" => include_bytes!(
                "../../calc-engine/tests/fixtures/excel-calc-probes/P14/eng10-20261104/inputs/E10-B6-boolean-n5-3ff0000000000000.xlsx"
            ),
            "E10-B6-boolean-n5-3ff0000000000001" => include_bytes!(
                "../../calc-engine/tests/fixtures/excel-calc-probes/P14/eng10-20261104/inputs/E10-B6-boolean-n5-3ff0000000000001.xlsx"
            ),
            "E10-B6-boolean-n5-0000000000000000" => include_bytes!(
                "../../calc-engine/tests/fixtures/excel-calc-probes/P14/eng10-20261104/inputs/E10-B6-boolean-n5-0000000000000000.xlsx"
            ),
            "E10-B6-error-n5-0000000000000000-peer-0" => include_bytes!(
                "../../calc-engine/tests/fixtures/excel-calc-probes/P14/eng10-20261104/inputs/E10-B6-error-n5-0000000000000000-peer-0.xlsx"
            ),
            "E10-B6-error-n5-0000000000000000-peer-1" => include_bytes!(
                "../../calc-engine/tests/fixtures/excel-calc-probes/P14/eng10-20261104/inputs/E10-B6-error-n5-0000000000000000-peer-1.xlsx"
            ),
            "E10-A2-Rebuild-eighth-0000000000000000" => include_bytes!(
                "../../calc-engine/tests/fixtures/excel-calc-probes/P14/eng10-20261104/inputs/E10-A2-Rebuild-eighth-0000000000000000.xlsx"
            ),
            "E10-A2-Rebuild-eighth-3fc0000000000000" => include_bytes!(
                "../../calc-engine/tests/fixtures/excel-calc-probes/P14/eng10-20261104/inputs/E10-A2-Rebuild-eighth-3fc0000000000000.xlsx"
            ),
            "E10-B6-error-n5-3f50624dd2f1a9fc-peer-0" => include_bytes!(
                "../../calc-engine/tests/fixtures/excel-calc-probes/P14/eng10-20261104/inputs/E10-B6-error-n5-3f50624dd2f1a9fc-peer-0.xlsx"
            ),
            _ => panic!("missing recorded input {name}"),
        };
        assert_eq!(
            calc_verdict::sha256_hex(bytes),
            result["input_sha256"].as_str().unwrap()
        );
        let observations = result["observations"].as_array().unwrap();
        let stage = |name: &str| observations.iter().find(|o| o["stage"] == name).unwrap();
        let seeds = stage("seeds-verified")["cells"].as_array().unwrap();
        for budget in [usize::MAX, 1] {
            let mut book = session_without_recalc(bytes);
            let iteration = book.host().model().definitions().iteration();
            assert!(iteration.enabled());
            assert_eq!(
                u64::from(iteration.max_iterations()),
                result["condition"]["iterations"].as_u64().unwrap()
            );
            assert_eq!(
                iteration.max_change().to_bits(),
                u64::from_str_radix(result["condition"]["change_bits"].as_str().unwrap(), 16)
                    .unwrap()
            );
            assert!(
                book.dispatch(SkinIntent::SetCalcMode(CalcMode::Manual))
                    .accepted()
            );
            book.set_calculation_budget(WorkBudget::evaluations(budget));
            let sheet = book.host().sheets()[0].id();
            let request = ProjectionRequest {
                cells: seeds
                    .iter()
                    .map(|c| {
                        CellKey::new(
                            sheet,
                            c["row"].as_u64().unwrap() as u32,
                            c["column"].as_u64().unwrap() as u32,
                        )
                        .unwrap()
                    })
                    .collect(),
            };
            let initial = project(book.host(), &request).unwrap();
            for (cell, seed) in initial.cells().iter().zip(seeds) {
                let ProjectedValue::FileCached(cache) = cell.value() else {
                    panic!("{name}: the seed is not calculated: {cell:?}");
                };
                assert_eq!(cache.value().to_value(), p14_value(&seed["value"]));
            }
            for command in 1..=result["condition"]["commands"].as_u64().unwrap() {
                assert!(book.dispatch(SkinIntent::Rebuild).accepted());
                let before = project(book.host(), &request).unwrap();
                let mut steps = 0;
                while book.has_pending_calculation() {
                    // DNA keeps iterative candidates private; Excel's intermediate
                    // caches are a separate unanswered observation question.
                    assert_eq!(
                        project(book.host(), &request).unwrap().cells(),
                        before.cells(),
                        "{name}"
                    );
                    assert!(
                        !matches!(book.advance_calculation(), HostProgress::Failed(_)),
                        "{name}"
                    );
                    steps += 1;
                    assert!(steps < 10000, "{name}");
                }
                let actual = project(book.host(), &request).unwrap();
                assert!(actual.recalc_failure().is_none(), "{name}");
                let expected = stage(&format!("after-command-{command}"))["cells"]
                    .as_array()
                    .unwrap();
                assert_eq!(actual.cells().len(), expected.len());
                for (cell, expected) in actual.cells().iter().zip(expected) {
                    let ProjectedValue::Calculated { value, calculation } = cell.value() else {
                        panic!("{name}: an evaluated result is required: {cell:?}");
                    };
                    assert!(!calculation.held_by_circular_rule());
                    // Iterative sessions still use the existing uncertified
                    // publication boundary. Matching final values does not
                    // supply a SliceV1 read certificate.
                    assert!(calculation.stamp().is_none(), "{name}");
                    assert_eq!(
                        value,
                        &p14_value(&expected["value"]),
                        "{name}, command {command}, budget {budget}, {:?}",
                        cell.at()
                    );
                    comparisons += 1;
                }
            }
        }
    }
    assert_eq!(comparisons, 180);
}

/// The delivered mixed Range commands leave selected cyclic/dependent caches
/// unchanged while the independent scalar D1 computes. Cache equality is never
/// counted as calculated work. The profile is Manual and StillRequired.
#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn dc_hsa5_11__p14_mixed_range_keeps_nonpasses() {
    use calc_host::value::GridArea;
    use calc_host::{HostProgress, WorkBudget};
    use serde_json::Value;
    let records = std::str::from_utf8(include_bytes!(
        "../../calc-engine/tests/fixtures/excel-calc-probes/P14/eng10-20261104/range-observations.jsonl"
    )).unwrap().lines().map(|line|serde_json::from_str::<Value>(line).unwrap()).collect::<Vec<_>>();
    assert_eq!(records.len(), 12);
    let mut comparisons = 0;
    let mut failures = Vec::new();
    let mut evaluated = 0;
    for record in records {
        let name = record["condition"].as_str().unwrap();
        let result = &record["result"];
        assert!(result["fatal"].is_null());
        let bytes: &[u8] = match name {
            "E10-Range-mixed-n1-zero" => include_bytes!(
                "../../calc-engine/tests/fixtures/excel-calc-probes/P14/eng10-20261104/inputs/E10-Range-mixed-n1-zero.xlsx"
            ),
            "E10-Range-all-readers-n1-zero" => include_bytes!(
                "../../calc-engine/tests/fixtures/excel-calc-probes/P14/eng10-20261104/inputs/E10-Range-all-readers-n1-zero.xlsx"
            ),
            "E10-Range-mixed-n1-asymmetric" => include_bytes!(
                "../../calc-engine/tests/fixtures/excel-calc-probes/P14/eng10-20261104/inputs/E10-Range-mixed-n1-asymmetric.xlsx"
            ),
            "E10-Range-all-readers-n1-asymmetric" => include_bytes!(
                "../../calc-engine/tests/fixtures/excel-calc-probes/P14/eng10-20261104/inputs/E10-Range-all-readers-n1-asymmetric.xlsx"
            ),
            "E10-Range-mixed-n2-zero" => include_bytes!(
                "../../calc-engine/tests/fixtures/excel-calc-probes/P14/eng10-20261104/inputs/E10-Range-mixed-n2-zero.xlsx"
            ),
            "E10-Range-all-readers-n2-zero" => include_bytes!(
                "../../calc-engine/tests/fixtures/excel-calc-probes/P14/eng10-20261104/inputs/E10-Range-all-readers-n2-zero.xlsx"
            ),
            "E10-Range-mixed-n2-asymmetric" => include_bytes!(
                "../../calc-engine/tests/fixtures/excel-calc-probes/P14/eng10-20261104/inputs/E10-Range-mixed-n2-asymmetric.xlsx"
            ),
            "E10-Range-all-readers-n2-asymmetric" => include_bytes!(
                "../../calc-engine/tests/fixtures/excel-calc-probes/P14/eng10-20261104/inputs/E10-Range-all-readers-n2-asymmetric.xlsx"
            ),
            "E10-Range-mixed-n5-zero" => include_bytes!(
                "../../calc-engine/tests/fixtures/excel-calc-probes/P14/eng10-20261104/inputs/E10-Range-mixed-n5-zero.xlsx"
            ),
            "E10-Range-all-readers-n5-zero" => include_bytes!(
                "../../calc-engine/tests/fixtures/excel-calc-probes/P14/eng10-20261104/inputs/E10-Range-all-readers-n5-zero.xlsx"
            ),
            "E10-Range-mixed-n5-asymmetric" => include_bytes!(
                "../../calc-engine/tests/fixtures/excel-calc-probes/P14/eng10-20261104/inputs/E10-Range-mixed-n5-asymmetric.xlsx"
            ),
            "E10-Range-all-readers-n5-asymmetric" => include_bytes!(
                "../../calc-engine/tests/fixtures/excel-calc-probes/P14/eng10-20261104/inputs/E10-Range-all-readers-n5-asymmetric.xlsx"
            ),
            _ => panic!("missing recorded range input {name}"),
        };
        assert_eq!(
            calc_verdict::sha256_hex(bytes),
            result["input_sha256"].as_str().unwrap()
        );
        let observations = result["observations"].as_array().unwrap();
        let stage = |name: &str| observations.iter().find(|o| o["stage"] == name).unwrap();
        let seeds = stage("seeds-verified")["cells"].as_array().unwrap();
        assert_eq!(seeds.len(), 5);
        let last = match result["condition"]["range"].as_str().unwrap() {
            "A1:D1" => 4,
            "A1:E1" => 5,
            other => panic!("unknown scope {other}"),
        };
        for budget in [usize::MAX, 1] {
            let mut book = session_without_recalc(bytes);
            assert!(
                book.dispatch(SkinIntent::SetCalcMode(CalcMode::Manual))
                    .accepted()
            );
            book.set_calculation_budget(WorkBudget::evaluations(budget));
            let sheet = book.host().sheets()[0].id();
            let at = |column| CellKey::new(sheet, 1, column).unwrap();
            let request = ProjectionRequest {
                cells: (1..=5).map(at).collect(),
            };
            for (cell, seed) in project(book.host(), &request)
                .unwrap()
                .cells()
                .iter()
                .zip(seeds)
            {
                let ProjectedValue::FileCached(cache) = cell.value() else {
                    panic!("{name}: initial seed must not be calculated: {cell:?}");
                };
                assert_eq!(cache.value().to_value(), p14_value(&seed["value"]));
            }
            for command in 1..=2 {
                assert!(
                    book.dispatch(SkinIntent::CalculateRange {
                        areas: vec![GridArea::new(at(1), at(last)).unwrap()]
                    })
                    .accepted()
                );
                let mut steps = 0;
                while book.has_pending_calculation() {
                    assert!(
                        !matches!(book.advance_calculation(), HostProgress::Failed(_)),
                        "{name}"
                    );
                    steps += 1;
                    assert!(steps < 1000, "{name}");
                }
                let actual = project(book.host(), &request).unwrap();
                let events = book.take_calculation_events();
                let admission = events
                    .iter()
                    .find_map(|event| match event {
                        calc_host::CalcEvent::Admitted(receipt) => Some(receipt),
                        _ => None,
                    })
                    .expect("the mixed command is admitted");
                let completion = events
                    .iter()
                    .find_map(|event| match event {
                        calc_host::CalcEvent::Completed(receipt) => Some(receipt),
                        _ => None,
                    })
                    .expect("the mixed command completes explicitly");
                assert_eq!(
                    admission.range_post_state,
                    calc_host::RangePostState::StillRequired
                );
                assert_eq!(completion.command, admission.command);
                assert_eq!(completion.outcome, calc_host::CompletionOutcome::Incomplete);
                assert_eq!(completion.coverage.required, last as usize);
                assert_eq!(completion.coverage.calculated, 1);
                assert_eq!(completion.coverage.refused, last as usize - 1);
                assert_eq!(completion.coverage.held, 0);
                assert_eq!(completion.coverage.uncertified, 0);
                let refused = p14_refused(&events, admission.command);
                let wanted_targets = (1..=last)
                    .filter(|column| *column != 4)
                    .map(at)
                    .collect::<std::collections::BTreeSet<_>>();
                assert_eq!(
                    refused,
                    wanted_targets
                );
                let expected = stage(&format!("after-command-{command}"))["cells"]
                    .as_array()
                    .unwrap();
                assert_eq!(expected.len(), 5);
                for (cell, expected) in actual.cells().iter().zip(expected) {
                    comparisons += 1;
                    let wanted = p14_value(&expected["value"]);
                    if cell.at().column() == 4 {
                        let calculation = match cell.value() {
                            ProjectedValue::Calculated { value, calculation }
                                if value == &wanted =>
                            {
                                Some(calculation)
                            }
                            ProjectedValue::Stale(stale) if stale.last_value() == Some(&wanted) => {
                                stale.last_calculation()
                            }
                            _ => None,
                        };
                        if let Some(calculation) = calculation {
                            assert!(!calculation.held_by_circular_rule());
                            let stamp = calculation
                                .stamp()
                                .expect("the independent scalar needs an ordinary certificate");
                            assert_eq!(stamp.certification.command, admission.command, "{name}");
                            assert!(
                                matches!(&stamp.computed_by, calc_engine::contract::ComputationOrigin::Evaluated { root } if root.pass.command == admission.command),
                                "{name}: ForceAll needs a new producing invocation"
                            );
                            evaluated += 1;
                        } else {
                            failures.push(format!("{name} command{command} budget{budget}: independent D1 must calculate {wanted:?}; got {:?}; failure {:?}",cell.value(),actual.recalc_failure()));
                        }
                    } else {
                        let cache = match cell.value() {
                            ProjectedValue::FileCached(cache) => cache,
                            ProjectedValue::Stale(stale) if stale.last_value().is_none() => {
                                stale.file_cache().unwrap()
                            }
                            other => panic!(
                                "{name}: retained/outside seed must remain an uncalculated nonpass: {other:?}"
                            ),
                        };
                        assert_eq!(cache.value().to_value(), wanted, "{name}");
                    }
                }
            }
        }
    }
    assert_eq!(comparisons, 240);
    assert!(failures.is_empty(), "{}", failures.join("\n"));
    assert_eq!(evaluated, 48);
}

/// DNA cancellation/supersession controls, rather than additional Excel observations.
#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn dc_hsa5_11__mixed_range_cancellation_and_supersession_keep_obligations() {
    use calc_host::value::GridArea;
    use calc_host::{CalcEvent, CompletionOutcome, WorkBudget};
    let bytes = include_bytes!(
        "../../calc-engine/tests/fixtures/excel-calc-probes/P14/eng10-20261104/inputs/E10-Range-mixed-n1-zero.xlsx"
    );
    for cancel in [true, false] {
        let mut book = session_without_recalc(bytes);
        assert!(
            book.dispatch(SkinIntent::SetCalcMode(CalcMode::Manual))
                .accepted()
        );
        p14_enter(&mut book, 1, 26, "10");
        p14_enter(&mut book, 1, 4, "=Z1+1");
        p14_enter(&mut book, 2, 4, "=Z1+2");
        p14_enter(&mut book, 3, 4, "=Z1+3");
        book.set_calculation_budget(WorkBudget::evaluations(1));
        let before = (1..=3)
            .map(|c| p14_visible(&book, 1, c))
            .collect::<Vec<_>>();
        book.take_calculation_events();
        let sheet = book.host().sheets()[0].id();
        let at = |row, column| CellKey::new(sheet, row, column).unwrap();
        assert!(
            book.dispatch(SkinIntent::CalculateRange {
                areas: vec![GridArea::new(at(1, 1), at(3, 4)).unwrap()]
            })
            .accepted()
        );
        assert!(
            book.has_pending_calculation(),
            "one evaluation cannot finish three independent targets"
        );
        if cancel {
            book.cancel_calculation();
            assert!(!book.has_pending_calculation());
        } else {
            p14_enter(&mut book, 1, 26, "20");
            p14_advance(&mut book);
        }
        let events = book.take_calculation_events();
        let admissions = events
            .iter()
            .filter_map(|event| match event {
                CalcEvent::Admitted(r) => Some(r.command),
                _ => None,
            })
            .collect::<Vec<_>>();
        let completions = events
            .iter()
            .filter_map(|event| match event {
                CalcEvent::Completed(r) | CalcEvent::Superseded(r) => Some(r),
                _ => None,
            })
            .collect::<Vec<_>>();
        assert_eq!(admissions.len(), if cancel { 1 } else { 2 });
        assert_eq!(completions.len(), admissions.len());
        for command in &admissions {
            assert_eq!(
                p14_refused(&events, *command),
                (1..=3).map(|column| at(1, column)).collect()
            );
        }
        let interrupted = completions
            .iter()
            .find(|r| r.command == admissions[0])
            .unwrap();
        assert_eq!(interrupted.coverage.required, 6);
        assert_eq!(interrupted.coverage.refused, 3);
        if cancel {
            assert_eq!(interrupted.outcome, CompletionOutcome::Cancelled);
        } else {
            assert!(matches!(
                interrupted.outcome,
                CompletionOutcome::Superseded { .. }
            ));
            let completed = completions
                .iter()
                .find(|r| r.command == admissions[1])
                .unwrap();
            assert_eq!(completed.outcome, CompletionOutcome::Incomplete);
            assert_eq!(completed.coverage.required, 6);
            assert_eq!(completed.coverage.calculated, 3);
            assert_eq!(completed.coverage.refused, 3);
            for row in 1..=3 {
                assert_eq!(
                    p14_visible(&book, row, 4).0,
                    CalcValue::number(20.0 + row as f64)
                );
            }
        }
        for (column, old) in (1..=3).zip(&before) {
            assert_eq!(p14_visible(&book, 1, column), *old);
        }
        // A subsequent authorized workbook command still consumes the retained seeds.
        assert!(book.dispatch(SkinIntent::Rebuild).accepted());
        p14_advance(&mut book);
        assert_eq!(p14_visible(&book, 1, 1).0, CalcValue::number(1.0));
        assert_eq!(p14_visible(&book, 1, 2).0, CalcValue::number(2.0));
    }
}

/// The measured partial command applies only to Manual scalar work. An array
/// discovered during execution must fail without installing an unmeasured spill.
#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn dc_hsa5_11__mixed_range_rejects_automatic_and_new_spills() {
    use calc_host::value::GridArea;
    let bytes = include_bytes!(
        "../../calc-engine/tests/fixtures/excel-calc-probes/P14/eng10-20261104/inputs/E10-Range-mixed-n1-zero.xlsx"
    );
    let mut automatic = session_without_recalc(bytes);
    assert!(
        automatic
            .dispatch(SkinIntent::SetCalcMode(CalcMode::Automatic))
            .accepted()
    );
    p14_advance(&mut automatic);
    let sheet = automatic.host().sheets()[0].id();
    let at = |c| CellKey::new(sheet, 1, c).unwrap();
    let receipt = automatic.dispatch(SkinIntent::CalculateRange {
        areas: vec![GridArea::new(at(1), at(4)).unwrap()],
    });
    assert!(matches!(
        receipt.recalc,
        Some(calc_host::HostProgress::Failed(
            calc_host::RecalcFailed::Refused(calc_engine::StartError::NotModeledScope {
                owner: "dc-hsa5.11",
                ..
            })
        ))
    ));

    let mut book = session_without_recalc(bytes);
    assert!(
        book.dispatch(SkinIntent::SetCalcMode(CalcMode::Manual))
            .accepted()
    );
    p14_enter(&mut book, 1, 26, "1");
    p14_enter(&mut book, 1, 4, "=SEQUENCE(Z1)");
    let before = p14_visible(&book, 1, 4);
    p14_enter(&mut book, 1, 26, "2");
    assert!(
        book.dispatch(SkinIntent::CalculateRange {
            areas: vec![GridArea::new(at(1), at(4)).unwrap()]
        })
        .accepted()
    );
    p14_advance(&mut book);
    let page = project(book.host(), &ProjectionRequest { cells: vec![at(4)] }).unwrap();
    assert!(
        matches!(
            page.recalc_failure(),
            Some(calc_host::RecalcFailed::Failed(
                calc_engine::RecalcFailure::NotModeledScope {
                    owner: "dc-hsa5.11",
                    ..
                }
            ))
        ),
        "{:?}",
        page.recalc_failure()
    );
    assert_eq!(p14_visible(&book, 1, 4), before);
}

/// A denied read must not replace a warm result before discovering that the
/// reader itself is cyclic. Its retained value is a later workbook seed.
#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn dc_hsa5_11__mixed_range_denial_preserves_hidden_cycle_seed() {
    let mut book = session_without_recalc(&calc_host::blank_package());
    assert!(
        book.dispatch(SkinIntent::SetCalcMode(CalcMode::Manual))
            .accepted()
    );
    set_iteration(&mut book, IterationSettings::new(true, 1, 0.001));
    p14_enter(&mut book, 1, 1, "=A1+1");
    p14_enter(&mut book, 1, 4, "=INDIRECT(\"A1\")+INDIRECT(\"D1\")");
    assert!(book.dispatch(SkinIntent::Rebuild).accepted());
    p14_advance(&mut book);
    let mut bytes = Vec::new();
    save(book.host(), &SavePolicy::default(), &mut bytes).unwrap();
    for budget in [usize::MAX, 1] {
        let mut book = session_without_recalc(&bytes);
        book.set_calculation_budget(calc_host::WorkBudget::evaluations(budget));
        let before = p14_visible(&book, 1, 4);
        let previous_a = p14_visible(&book, 1, 1).0.as_number().unwrap();
        let sheet = book.host().sheets()[0].id();
        let at = |c| CellKey::new(sheet, 1, c).unwrap();
        assert!(
            book.dispatch(SkinIntent::CalculateRange {
                areas: vec![calc_host::value::GridArea::new(at(1), at(4)).unwrap()]
            })
            .accepted()
        );
        p14_advance(&mut book);
        let page = project(book.host(), &ProjectionRequest { cells: vec![at(4)] }).unwrap();
        assert!(page.recalc_failure().is_none());
        let events = book.take_calculation_events();
        let done = events.iter().find_map(|e| match e {
            calc_host::CalcEvent::Completed(r) => Some(r), _ => None,
        }).unwrap();
        assert_eq!(done.outcome, calc_host::CompletionOutcome::Incomplete);
        assert_eq!((done.coverage.required, done.coverage.calculated, done.coverage.refused), (2, 0, 2));
        assert_eq!(p14_refused(&events, done.command), [at(1), at(4)].into_iter().collect());
        assert_eq!(p14_visible(&book, 1, 4), before);
        assert!(book.dispatch(SkinIntent::Rebuild).accepted());
        p14_advance(&mut book);
        assert_eq!(
            p14_visible(&book, 1, 4).0,
            CalcValue::number(before.0.as_number().unwrap() + previous_a + 1.0)
        );
    }
}

/// An unknown collision must refuse before replacing the scalar's warm result.
#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn dc_hsa5_11__mixed_range_unknown_array_collision_keeps_seed() {
    use calc_host::value::GridArea;
    let bytes = include_bytes!(
        "../../calc-engine/tests/fixtures/excel-calc-probes/P14/eng10-20261104/inputs/E10-Range-mixed-n1-zero.xlsx"
    );
    for budget in [usize::MAX, 1] {
        let mut book = session_without_recalc(bytes);
        assert!(
            book.dispatch(SkinIntent::SetCalcMode(CalcMode::Manual))
                .accepted()
        );
        p14_enter(&mut book, 1, 26, "1");
        p14_enter(&mut book, 1, 4, "=SEQUENCE(Z1)");
        let before = p14_visible(&book, 1, 4);
        let sheet = book.host().sheets()[0].id();
        let at = |r, c| CellKey::new(sheet, r, c).unwrap();
        // The outside anchor stays uncalculated; only a potential spill child meets D2.
        assert!(
            book.dispatch(SkinIntent::Edit {
                edits: vec![CellEdit::SetArray {
                    array: calc_host::ArrayFormula::new(
                        GridArea::new(at(2, 3), at(2, 5)).unwrap(),
                        calc_host::ArrayKind::Dynamic { collapsed: false }
                    ),
                    formula: calc_host::FormulaText::new(
                        calc_host::value::ExcelText::from_interop_assignment("SEQUENCE(1,3)")
                    )
                    .unwrap(),
                }]
            })
            .accepted()
        );
        p14_enter(&mut book, 1, 26, "2");
        book.set_calculation_budget(calc_host::WorkBudget::evaluations(budget));
        assert!(
            book.dispatch(SkinIntent::CalculateRange {
                areas: vec![GridArea::new(at(1, 1), at(1, 4)).unwrap()]
            })
            .accepted()
        );
        p14_advance(&mut book);
        let page = project(
            book.host(),
            &ProjectionRequest {
                cells: vec![at(1, 4)],
            },
        )
        .unwrap();
        assert!(
            matches!(
                page.recalc_failure(),
                Some(calc_host::RecalcFailed::Failed(
                    calc_engine::RecalcFailure::NotModeledScope {
                        owner: "dc-hsa5.11",
                        ..
                    }
                ))
            ),
            "{:?}",
            page.recalc_failure()
        );
        assert_eq!(p14_visible(&book, 1, 4), before);
    }
}

/// A missing outside cache can stop a reader before its own self-reference is
/// discovered. This DNA integrity control preserves the next workbook seed.
#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn dc_hsa5_11__mixed_range_boundary_refusal_keeps_hidden_cycle_seed() {
    let mut book = session_without_recalc(&calc_host::blank_package());
    assert!(
        book.dispatch(SkinIntent::SetCalcMode(CalcMode::Manual))
            .accepted()
    );
    set_iteration(&mut book, IterationSettings::new(true, 1, 0.001));
    p14_enter(&mut book, 1, 26, "TRUE");
    p14_enter(&mut book, 1, 1, "=A1+1");
    p14_enter(
        &mut book,
        1,
        4,
        "=IF(Z1,1,INDIRECT(\"E1\"))+INDIRECT(\"D1\")",
    );
    assert!(book.dispatch(SkinIntent::Rebuild).accepted());
    p14_advance(&mut book);
    let mut bytes = Vec::new();
    save(book.host(), &SavePolicy::default(), &mut bytes).unwrap();
    for budget in [usize::MAX, 1] {
        let mut book = session_without_recalc(&bytes);
        book.set_calculation_budget(calc_host::WorkBudget::evaluations(budget));
        let previous = p14_visible(&book, 1, 4).0;
        let sheet = book.host().sheets()[0].id();
        let at = |c| CellKey::new(sheet, 1, c).unwrap();
        // Author E1 without entry calculation, outside the admitted A1:D1 scope.
        assert!(
            book.dispatch(SkinIntent::Edit {
                edits: vec![CellEdit::Set {
                    at: at(5),
                    input: calc_host::CellInput::Formula(
                        calc_host::FormulaText::new(
                            calc_host::value::ExcelText::from_interop_assignment("1+1")
                        )
                        .unwrap()
                    ),
                }]
            })
            .accepted()
        );
        p14_enter(&mut book, 1, 26, "FALSE");
        assert!(
            book.dispatch(SkinIntent::CalculateRange {
                areas: vec![calc_host::value::GridArea::new(at(1), at(4)).unwrap()]
            })
            .accepted()
        );
        p14_advance(&mut book);
        let page = project(book.host(), &ProjectionRequest { cells: vec![at(4)] }).unwrap();
        assert!(matches!(
            page.recalc_failure(),
            Some(calc_host::RecalcFailed::Failed(
                calc_engine::RecalcFailure::NotModeledScope {
                    owner: "dc-hsa5.11",
                    ..
                }
            ))
        ));
        assert_eq!(p14_visible(&book, 1, 4).0, previous);
        p14_enter(&mut book, 1, 26, "TRUE");
        assert!(book.dispatch(SkinIntent::Rebuild).accepted());
        p14_advance(&mut book);
        assert_eq!(
            p14_visible(&book, 1, 4).0,
            CalcValue::number(previous.as_number().unwrap() + 1.0)
        );
    }
}

fn p14_advance(book: &mut HostSession) {
    let mut steps = 0;
    while book.has_pending_calculation() {
        book.advance_calculation();
        steps += 1;
        assert!(steps < 10000, "finite P14 work must terminate");
    }
}

fn p14_refused(events: &[calc_host::CalcEvent], command: calc_engine::contract::CalcCommandId)
    -> std::collections::BTreeSet<CellKey>
{
    let mut refused = std::collections::BTreeSet::new();
    for event in events {
        if let calc_host::CalcEvent::TargetsRefused { command: held, targets, owner, .. } = event
            && *held == command {
            assert_eq!(*owner, "dc-hsa5.11");
            for target in targets { assert!(refused.insert(*target), "duplicate refusal {target:?}"); }
        }
    }
    refused
}

fn p14_enter(book: &mut HostSession, row: u32, column: u32, text: &str) {
    let at = CellKey::new(book.host().sheets()[0].id(), row, column).unwrap();
    assert!(
        book.dispatch(SkinIntent::EnterCell {
            at,
            text: text.into()
        })
        .accepted()
    );
}

fn p14_visible(
    book: &HostSession,
    row: u32,
    column: u32,
) -> (CalcValue, Option<calc_host::Calculation>) {
    let at = CellKey::new(book.host().sheets()[0].id(), row, column).unwrap();
    let page = project(book.host(), &ProjectionRequest { cells: vec![at] }).unwrap();
    match page.cells()[0].value() {
        ProjectedValue::Calculated { value, calculation } => {
            (value.clone(), Some(calculation.clone()))
        }
        ProjectedValue::Stale(stale) if stale.last_value().is_some() => (
            stale.last_value().unwrap().clone(),
            stale.last_calculation().cloned(),
        ),
        ProjectedValue::FileCached(cache) => (cache.value().to_value(), None),
        ProjectedValue::Stale(stale) if stale.file_cache().is_some() => {
            (stale.file_cache().unwrap().value().to_value(), None)
        }
        other => panic!("no displayed value at {at:?}: {other:?}; failure {:?}", page.recalc_failure()),
    }
}

/// DNA lifecycle control, not an Excel execution-count observation. Saved dynamic
/// edges are rediscovered through the same evaluator, including a reader waiting
/// on a later reader that is then excluded. Exercise sparse and dense scratch.
#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn dc_hsa5_11__range_exclusion_wakes_waiters_and_cancel_counts_only_discovered_refusals() {
    use calc_host::{CalcEvent, CompletionOutcome, WorkBudget};
    use calc_host::value::GridArea;
    for (sparse, dynamic_cycle) in [(false, false), (true, false), (false, true), (true, true)] {
        let source = if dynamic_cycle { "eng14-range-dynamic-cycle" } else { "eng14-range-dynamic-cycle-reader" };
        let mut authored = HostSession::open(Arc::new(MemorySource::new(p14_input(source).to_vec())),
            &OpenPolicy::default(), HostOptions { calc_mode: Some(CalcMode::Manual), ..HostOptions::default() }).unwrap();
        p14_enter(&mut authored, 1, 3, "=INDIRECT(\"E1\")+1");
        p14_enter(&mut authored, 1, 5, "=INDIRECT(\"A1\")+1");
        if sparse { for row in 100..400 { p14_enter(&mut authored, row, 10, "=2+2"); } }
        assert!(authored.dispatch(SkinIntent::Rebuild).accepted());
        p14_advance(&mut authored);
        let mut bytes = Vec::new();
        save(authored.host(), &SavePolicy::default(), &mut bytes).unwrap();
        let sheet = authored.host().sheets()[0].id();
        let key = |column| CellKey::new(sheet, 1, column).unwrap();
        let warm = [3, 5].map(|column| (p14_visible(&authored, 1, column), authored.host().precedents(key(column))));
        assert!(authored.dispatch(SkinIntent::CalculateRange { areas: vec![GridArea::new(key(1), key(5)).unwrap()] }).accepted());
        p14_advance(&mut authored);
        for (column, before) in [3, 5].into_iter().zip(warm) {
            assert_eq!(p14_visible(&authored, 1, column), before.0, "warm identity survives exclusion");
            assert_eq!(authored.host().precedents(key(column)), before.1, "dynamic topology survives exclusion");
        }
        let cancellations = if dynamic_cycle { vec![None, Some(0), Some(2), Some(3)] } else { vec![None, Some(2), Some(3)] };
        for cancel_after in cancellations {
            let mut book = HostSession::open(Arc::new(MemorySource::new(bytes.clone())), &OpenPolicy::default(),
                HostOptions { calc_mode: Some(CalcMode::Manual), ..HostOptions::default() }).unwrap();
            book.set_calculation_budget(WorkBudget::evaluations(1));
            let sheet = book.host().sheets()[0].id();
            let at = |column| CellKey::new(sheet, 1, column).unwrap();
            let before = (1..=5).map(|column| p14_visible(&book, 1, column)).collect::<Vec<_>>();
            book.take_calculation_events();
            assert!(book.dispatch(SkinIntent::CalculateRange { areas: vec![GridArea::new(at(1), at(5)).unwrap()] }).accepted());
            let mut events = book.take_calculation_events();
            let command = events.iter().find_map(|e| match e { CalcEvent::Admitted(r) => Some(r.command), _ => None }).unwrap();
            let initial = if dynamic_cycle { std::collections::BTreeSet::new() } else { [at(1), at(2)].into_iter().collect() };
            assert_eq!(p14_refused(&events, command), initial, "dynamic cycles are refused only after their actual closed wait chain");
            let mut steps = 0;
            while book.has_pending_calculation() {
                if cancel_after.is_some_and(|count| p14_refused(&events, command).len() == count) {
                    book.cancel_calculation();
                    events.extend(book.take_calculation_events());
                    break;
                }
                book.advance_calculation();
                events.extend(book.take_calculation_events());
                steps += 1;
                assert!(steps < 100, "an excluded producer must release every waiter");
            }
            let done = events.iter().find_map(|e| match e { CalcEvent::Completed(r) => Some(r), _ => None }).unwrap();
            assert_eq!(done.coverage.required, 5);
            assert_eq!(done.coverage.refused, cancel_after.unwrap_or(4));
            assert_eq!(p14_refused(&events, command).len(), done.coverage.refused);
            assert_eq!(done.outcome, if cancel_after.is_some() { CompletionOutcome::Cancelled } else { CompletionOutcome::Incomplete });
            for column in 1..=5 {
                if column != 4 || cancel_after.is_some() {
                    assert_eq!(p14_visible(&book, 1, column), before[(column - 1) as usize], "retain the original seed and identity");
                }
            }
            if cancel_after.is_none() {
                assert_eq!(done.coverage.calculated, 1);
                let result = p14_visible(&book, 1, 4);
                assert_eq!(result.0, CalcValue::number(4.));
                assert_eq!(result.1.unwrap().stamp().unwrap().certification.command, command);
            }
        }
    }
}

/// Permitted shape metadata must not grant the selected cyclic value, including
/// after a yield or through IFERROR. These are DNA read-boundary controls.
#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn dc_hsa5_11__selected_scalar_shape_does_not_grant_its_value() {
    use calc_host::{CalcEvent, CompletionOutcome, WorkBudget};
    use calc_host::value::GridArea;
    for formula in ["=SUM(A1#)", "=INDEX(A1#,1)", "=A1#", "=ROWS(A1#)+A1", "=IFERROR(SUM(A1#),777)"] {
        for budget in [usize::MAX, 1] {
            let mut book = HostSession::open(Arc::new(MemorySource::new(p14_input("eng14-range-cyclic-shape-reader").to_vec())),
                &OpenPolicy::default(), HostOptions { calc_mode: Some(CalcMode::Manual), ..HostOptions::default() }).unwrap();
            p14_enter(&mut book, 1, 3, formula);
            let before = [p14_visible(&book, 1, 1), p14_visible(&book, 1, 2)];
            book.set_calculation_budget(WorkBudget::evaluations(budget));
            let sheet = book.host().sheets()[0].id();
            let at = |column| CellKey::new(sheet, 1, column).unwrap();
            book.take_calculation_events();
            assert!(book.dispatch(SkinIntent::CalculateRange { areas: vec![GridArea::new(at(1), at(4)).unwrap()] }).accepted());
            p14_advance(&mut book);
            let events = book.take_calculation_events();
            let done = events.iter().find_map(|e| match e { CalcEvent::Completed(r) => Some(r), _ => None }).unwrap();
            assert_eq!(done.outcome, CompletionOutcome::Incomplete);
            assert_eq!((done.coverage.required, done.coverage.calculated, done.coverage.refused), (4, 1, 3));
            assert_eq!(p14_refused(&events, done.command), [at(1), at(2), at(3)].into_iter().collect());
            let page = project(book.host(), &ProjectionRequest { cells: vec![at(3)] }).unwrap();
            match page.cells()[0].value() {
                ProjectedValue::Calculated { .. } => panic!("{formula}: a denied value cannot calculate"),
                ProjectedValue::Stale(stale) => assert!(stale.last_calculation().is_none()),
                _ => {}
            }
            assert_eq!(p14_visible(&book, 1, 1), before[0]);
            assert_eq!(p14_visible(&book, 1, 2), before[1]);
            let independent = p14_visible(&book, 1, 4);
            assert_eq!(independent.0, CalcValue::number(4.));
            assert_eq!(independent.1.unwrap().stamp().unwrap().certification.command, done.command);
        }
    }
}

/// DNA lifecycle controls: an unsupported range must not consume a warm cyclic
/// value, lose its seeds/topology, or discharge another component's future demand.
#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn dc_hsa5_11__mixed_range_preserves_warm_cycles_and_read_guards() {
    use calc_host::value::GridArea;
    use calc_host::{CalcEvent, CompletionOutcome, WorkBudget};
    let bytes = include_bytes!(
        "../../calc-engine/tests/fixtures/excel-calc-probes/P14/eng10-20261104/inputs/E10-Range-mixed-n1-zero.xlsx"
    );
    for budget in [usize::MAX, 1] {
        let mut book = session_without_recalc(bytes);
        assert!(
            book.dispatch(SkinIntent::SetCalcMode(CalcMode::Manual))
                .accepted()
        );
        book.set_calculation_budget(WorkBudget::evaluations(budget));
        assert!(book.dispatch(SkinIntent::Rebuild).accepted());
        p14_advance(&mut book);
        assert_eq!(p14_visible(&book, 1, 1).0, CalcValue::number(1.0));
        assert_eq!(p14_visible(&book, 1, 2).0, CalcValue::number(2.0));
        p14_enter(&mut book, 1, 26, "3");
        p14_enter(&mut book, 1, 4, "=Z1+1");
        p14_enter(&mut book, 1, 6, "=INDIRECT(\"A1\")+1");
        p14_enter(&mut book, 1, 7, "=ROWS(INDIRECT(\"A1\")#)");
        p14_enter(&mut book, 1, 26, "10");
        let before = (1..=3)
            .map(|c| p14_visible(&book, 1, c))
            .collect::<Vec<_>>();
        book.take_calculation_events();
        let sheet = book.host().sheets()[0].id();
        let at = |column| CellKey::new(sheet, 1, column).unwrap();
        assert!(
            book.dispatch(SkinIntent::CalculateRange {
                areas: vec![GridArea::new(at(1), at(4)).unwrap()]
            })
            .accepted()
        );
        p14_advance(&mut book);
        for (column, old) in (1..=3).zip(&before) {
            let shown = p14_visible(&book, 1, column);
            assert_eq!(shown.0, old.0);
            assert_eq!(
                shown.1.as_ref().map(|c| c.tick()),
                old.1.as_ref().map(|c| c.tick()),
                "a retained result must not acquire a new calculation"
            );
        }
        assert_eq!(p14_visible(&book, 1, 4).0, CalcValue::number(11.0));
        let events = book.take_calculation_events();
        let completion = events
            .iter()
            .find_map(|e| {
                if let CalcEvent::Completed(r) = e {
                    Some(r)
                } else {
                    None
                }
            })
            .unwrap();
        assert_eq!(completion.outcome, CompletionOutcome::Incomplete);
        assert_eq!(completion.coverage.required, 4);
        assert_eq!(completion.coverage.calculated, 1);
        assert_eq!(completion.coverage.refused, 3);
        assert!(!book.has_pending_calculation());
        for column in [6, 7] {
            assert!(
                book.dispatch(SkinIntent::CalculateRange {
                    areas: vec![
                        GridArea::new(at(1), at(4)).unwrap(),
                        GridArea::cell(at(column))
                    ]
                })
                .accepted()
            );
            p14_advance(&mut book);
            let page = project(
                book.host(),
                &ProjectionRequest {
                    cells: vec![at(column)],
                },
            )
            .unwrap();
            assert!(page.recalc_failure().is_none(), "column {column}: {:?}", page.recalc_failure());
            if column == 6 {
                assert!(!matches!(page.cells()[0].value(), ProjectedValue::Calculated { .. }));
                let events = book.take_calculation_events();
                let done = events.iter().find_map(|e| match e { CalcEvent::Completed(r) => Some(r), _ => None }).unwrap();
                assert_eq!(done.outcome, CompletionOutcome::Incomplete);
                assert_eq!(done.coverage.refused, 4);
                assert_eq!(p14_refused(&events, done.command), [at(1), at(2), at(3), at(6)].into_iter().collect());
            } else {
                let shown = p14_visible(&book, 1, column);
                assert_eq!(shown.0, CalcValue::number(1.0));
                let calculation = shown.1.unwrap();
                let stamp = calculation.stamp().unwrap();
                assert!(stamp.reads.uses_boundary_cache);
                assert!(!stamp.reads.uses_imported_cache);
                assert!(matches!(stamp.computed_by, calc_engine::contract::ComputationOrigin::Evaluated { .. }));
            }
            for (c, old) in (1..=3).zip(&before) {
                assert_eq!(p14_visible(&book, 1, c), *old);
            }
        }
        assert!(book.dispatch(SkinIntent::Rebuild).accepted());
        p14_advance(&mut book);
        assert_eq!(p14_visible(&book, 1, 1).0, CalcValue::number(3.0));
        assert_eq!(p14_visible(&book, 1, 2).0, CalcValue::number(4.0));
    }
    // A narrow completion must retain a clean component outside its scope for F9.
    let mut book = session_without_recalc(bytes);
    assert!(
        book.dispatch(SkinIntent::SetCalcMode(CalcMode::Manual))
            .accepted()
    );
    p14_enter(&mut book, 1, 24, "=Y1+1");
    p14_enter(&mut book, 1, 25, "=X1+1");
    assert!(book.dispatch(SkinIntent::Rebuild).accepted());
    p14_advance(&mut book);
    let y = p14_visible(&book, 1, 25).0.as_number().unwrap();
    let sheet = book.host().sheets()[0].id();
    let at = |column| CellKey::new(sheet, 1, column).unwrap();
    assert!(
        book.dispatch(SkinIntent::CalculateRange {
            areas: vec![GridArea::new(at(1), at(4)).unwrap()]
        })
        .accepted()
    );
    p14_advance(&mut book);
    assert_eq!(p14_visible(&book, 1, 25).0, CalcValue::number(y));
    assert!(book.dispatch(SkinIntent::Recalculate).accepted());
    p14_advance(&mut book);
    assert_eq!(p14_visible(&book, 1, 24).0, CalcValue::number(y + 1.0));
    assert_eq!(p14_visible(&book, 1, 25).0, CalcValue::number(y + 2.0));
}

/// Refusing a newly discovered dynamic cycle preserves its saved warm seed for
/// a later ordinary command. This is a DNA refusal/provenance control.
#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn dc_hsa5_11__dynamic_range_refusal_keeps_warm_seed() {
    use calc_host::WorkBudget;
    use calc_host::value::GridArea;
    for through_name in [false, true] {
        let mut authored = session_without_recalc(&calc_host::blank_package());
        assert!(
            authored
                .dispatch(SkinIntent::SetCalcMode(CalcMode::Manual))
                .accepted()
        );
        set_iteration(&mut authored, IterationSettings::new(true, 1, 0.001));
        if through_name {
            assert!(
                authored
                    .dispatch(SkinIntent::Edit {
                        edits: vec![CellEdit::SetName {
                            name: "DynamicSelf".into(),
                            scope: calc_host::value::NameScope::Workbook,
                            formula: Some("=INDIRECT(\"A1\")".into()),
                        }]
                    })
                    .accepted()
            );
        }
        p14_enter(
            &mut authored,
            1,
            1,
            if through_name {
                "=DynamicSelf+1"
            } else {
                "=INDIRECT(\"A1\")+1"
            },
        );
        p14_enter(&mut authored, 1, 4, "=2+2");
        assert!(authored.dispatch(SkinIntent::Rebuild).accepted());
        p14_advance(&mut authored);
        let held = p14_visible(&authored, 1, 1).0;
        let mut bytes = Vec::new();
        save(authored.host(), &SavePolicy::default(), &mut bytes).unwrap();
        for budget in [usize::MAX, 1] {
            let mut book = session_without_recalc(&bytes);
            assert!(
                book.dispatch(SkinIntent::SetCalcMode(CalcMode::Manual))
                    .accepted()
            );
            book.set_calculation_budget(WorkBudget::evaluations(budget));
            assert_eq!(p14_visible(&book, 1, 1).0, held);
            let sheet = book.host().sheets()[0].id();
            let at = |column| CellKey::new(sheet, 1, column).unwrap();
            assert!(
                book.dispatch(SkinIntent::CalculateRange {
                    areas: vec![GridArea::new(at(1), at(4)).unwrap()]
                })
                .accepted()
            );
            p14_advance(&mut book);
            let page = project(book.host(), &ProjectionRequest { cells: vec![at(1)] }).unwrap();
            assert!(
                page.recalc_failure().is_none(),
                "{:?}",
                page.recalc_failure()
            );
            let independent = p14_visible(&book, 1, 4);
            assert_eq!(independent.0, CalcValue::number(4.0));
            let stamp = independent.1.unwrap().stamp().unwrap().clone();
            assert!(matches!(
                stamp.computed_by,
                calc_engine::contract::ComputationOrigin::Evaluated { .. }
            ));
            assert!(!stamp.reads.uses_imported_cache && !stamp.reads.uses_boundary_cache);
            let events = book.take_calculation_events();
            let completed = events
                .iter()
                .find_map(|event| match event {
                    calc_host::CalcEvent::Completed(receipt) => Some(receipt),
                    _ => None,
                })
                .unwrap();
            assert_eq!(completed.outcome, calc_host::CompletionOutcome::Incomplete);
            assert_eq!(
                (
                    completed.coverage.required,
                    completed.coverage.calculated,
                    completed.coverage.refused
                ),
                (2, 1, 1)
            );
            assert_eq!(stamp.certification.command, completed.command);
            assert_eq!(p14_visible(&book, 1, 1).0, held);
            assert!(
                p14_visible(&book, 1, 1).1.is_none(),
                "refusal cannot create a producing calculation"
            );
            assert!(book.dispatch(SkinIntent::Rebuild).accepted());
            p14_advance(&mut book);
            assert_eq!(
                p14_visible(&book, 1, 1).0,
                CalcValue::number(held.as_number().unwrap() + 1.0)
            );
        }
    }
}

/// DNA scope control: unsupported providers/spills keep a whole-command refusal,
/// while a normal worksheet #NAME? remains a calculated scalar error.
#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn dc_hsa5_11__dynamic_range_neighbors_keep_scope_and_value_errors_distinct() {
    use calc_host::WorkBudget;
    use calc_host::value::GridArea;
    for neighbor in ["=SEQUENCE(2,1)", "=MissingPacketFunction()", "='missing-provider.xlsm'!NoSuch(1)"] {
        for budget in [usize::MAX, 1] {
            let mut book = HostSession::open(Arc::new(MemorySource::new(
                p14_input("eng14-range-dynamic-cycle").to_vec())), &OpenPolicy::default(),
                HostOptions { calc_mode: Some(CalcMode::Manual), ..HostOptions::default() }).unwrap();
            p14_enter(&mut book, 1, 3, neighbor);
            book.set_calculation_budget(WorkBudget::evaluations(budget));
            let sheet = book.host().sheets()[0].id();
            let at = |column| CellKey::new(sheet, 1, column).unwrap();
            // Bind the original static references without evaluating A/B/C.
            // Cold None versus newly bound Some([]) is not a topology change.
            assert!(book.dispatch(SkinIntent::CalculateRange {
                areas: vec![GridArea::cell(at(4))]
            }).accepted());
            p14_advance(&mut book);
            book.take_calculation_events();
            let before = [1, 2, 4].map(|column|
                (p14_visible(&book, 1, column), book.host().precedents(at(column))));
            assert!(book.dispatch(SkinIntent::CalculateRange {
                areas: vec![GridArea::new(at(1), at(4)).unwrap()]
            }).accepted());
            p14_advance(&mut book);
            let page = project(book.host(), &ProjectionRequest { cells: vec![at(4)] }).unwrap();
            if neighbor == "=MissingPacketFunction()" {
                assert!(page.recalc_failure().is_none());
                let ordinary = p14_visible(&book, 1, 3);
                assert_eq!(ordinary.0.as_error(), Some(calc_host::value::WorksheetErrorCode::Name));
                let events = book.take_calculation_events();
                let completed = events.iter().find_map(|event| match event {
                    calc_host::CalcEvent::Completed(receipt) => Some(receipt), _ => None,
                }).unwrap();
                assert_eq!((completed.coverage.required, completed.coverage.calculated, completed.coverage.refused), (4, 2, 2));
                assert_eq!(completed.outcome, calc_host::CompletionOutcome::Incomplete);
                for column in [3, 4] {
                    assert_eq!(p14_visible(&book, 1, column).1.unwrap().stamp().unwrap().certification.command, completed.command);
                }
                for (column, original) in [1, 2].into_iter().zip(&before) {
                    assert_eq!(p14_visible(&book, 1, column), original.0);
                    assert_eq!(book.host().precedents(at(column)), original.1);
                }
                continue;
            }
            assert!(matches!(page.recalc_failure(), Some(calc_host::RecalcFailed::Failed(
                calc_engine::RecalcFailure::NotModeledScope { owner: "dc-hsa5.11", .. }
            ))), "{neighbor}: {:?}", page.recalc_failure());
            let events = book.take_calculation_events();
            assert!(!events.iter().any(|event| matches!(event, calc_host::CalcEvent::Published(_))));
            if neighbor.starts_with("='missing-provider") {
                assert!(!events.iter().any(|event| matches!(event, calc_host::CalcEvent::TargetsRefused { .. })),
                    "the unsupported participant must reject partial admission before producer exclusion");
            }
            for (column, original) in [1, 2, 4].into_iter().zip(before) {
                assert_eq!(p14_visible(&book, 1, column), original.0);
                assert_eq!(book.host().precedents(at(column)), original.1);
            }
        }
    }
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

/// P14's pure COM recordings (Excel 16.0.20527, 2026-11-03): two distinct
/// commands from verified stored seeds, strict threshold equality, independent
/// cycles and downstream amplifiers. At least three trials per condition share
/// processes within the recorded groups; these are not fresh-process replicates. There are no UDF
/// observers of mid-cycle caches in this delivery.
#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn dc_hsa5_11__p14_recorded_stopping_and_dependents() {
    use calc_host::value::GridArea;
    use calc_host::{HostProgress, WorkBudget};
    use serde_json::Value;
    use std::collections::BTreeMap;

    let records = std::str::from_utf8(include_bytes!(
        "../../calc-engine/tests/fixtures/excel-calc-probes/P14/eng9-20261103/observations.jsonl"
    ))
    .unwrap()
    .lines()
    .map(|line| serde_json::from_str::<Value>(line).unwrap())
    .collect::<Vec<_>>();
    let mut groups = BTreeMap::<&str, Vec<&Value>>::new();
    for record in &records {
        groups
            .entry(record["condition"].as_str().unwrap())
            .or_default()
            .push(record);
    }
    assert_eq!(groups.len(), 86);
    assert_eq!(records.len(), 692);
    let mut failures = Vec::new();
    let mut refused_range = Vec::new();
    let mut commands = 0;
    let mut comparisons = 0;
    let mut refused_values = 0;
    for (name, trials) in groups {
        assert!(trials.len() >= 3, "{name}");
        let result = &trials[0]["result"];
        let condition = &result["condition"];
        let observations = result["observations"].as_array().unwrap();
        // Do not hide trial disagreement by choosing the first trial's values.
        for trial in &trials {
            assert!(trial["result"]["fatal"].is_null(), "{name}");
            assert_eq!(
                trial["result"]["input_sha256"], result["input_sha256"],
                "{name}"
            );
            let observed = trial["result"]["observations"].as_array().unwrap();
            assert_eq!(observed.len(), observations.len(), "{name}");
            for (a, b) in observed.iter().zip(observations) {
                assert_eq!(a["stage"], b["stage"], "{name}");
                assert_eq!(a["cells"], b["cells"], "{name} {}", a["stage"]);
            }
        }
        if condition["operation"] == "Range" {
            refused_range.push(name);
        }
        let bytes: &[u8] = match name {
            "A1-sweeps-1-asymmetric" => include_bytes!("../../calc-engine/tests/fixtures/excel-calc-probes/P14/eng9-20261103/conditions/A1-sweeps-1-asymmetric/trial-01/input.xlsx"),
            "A1-sweeps-1-asymmetric-Calculate" => include_bytes!("../../calc-engine/tests/fixtures/excel-calc-probes/P14/eng9-20261103/conditions/A1-sweeps-1-asymmetric-Calculate/trial-01/input.xlsx"),
            "A1-sweeps-1-asymmetric-Full" => include_bytes!("../../calc-engine/tests/fixtures/excel-calc-probes/P14/eng9-20261103/conditions/A1-sweeps-1-asymmetric-Full/trial-01/input.xlsx"),
            "A1-sweeps-1-asymmetric-Range" => include_bytes!("../../calc-engine/tests/fixtures/excel-calc-probes/P14/eng9-20261103/conditions/A1-sweeps-1-asymmetric-Range/trial-01/input.xlsx"),
            "A1-sweeps-1-asymmetric-Sheet" => include_bytes!("../../calc-engine/tests/fixtures/excel-calc-probes/P14/eng9-20261103/conditions/A1-sweeps-1-asymmetric-Sheet/trial-01/input.xlsx"),
            "A1-sweeps-1-asymmetric-reverse" => include_bytes!("../../calc-engine/tests/fixtures/excel-calc-probes/P14/eng9-20261103/conditions/A1-sweeps-1-asymmetric-reverse/trial-01/input.xlsx"),
            "A1-sweeps-1-zero" => include_bytes!("../../calc-engine/tests/fixtures/excel-calc-probes/P14/eng9-20261103/conditions/A1-sweeps-1-zero/trial-01/input.xlsx"),
            "A1-sweeps-1-zero-Calculate" => include_bytes!("../../calc-engine/tests/fixtures/excel-calc-probes/P14/eng9-20261103/conditions/A1-sweeps-1-zero-Calculate/trial-01/input.xlsx"),
            "A1-sweeps-1-zero-Full" => include_bytes!("../../calc-engine/tests/fixtures/excel-calc-probes/P14/eng9-20261103/conditions/A1-sweeps-1-zero-Full/trial-01/input.xlsx"),
            "A1-sweeps-1-zero-Range" => include_bytes!("../../calc-engine/tests/fixtures/excel-calc-probes/P14/eng9-20261103/conditions/A1-sweeps-1-zero-Range/trial-01/input.xlsx"),
            "A1-sweeps-1-zero-Sheet" => include_bytes!("../../calc-engine/tests/fixtures/excel-calc-probes/P14/eng9-20261103/conditions/A1-sweeps-1-zero-Sheet/trial-01/input.xlsx"),
            "A1-sweeps-1-zero-reverse" => include_bytes!("../../calc-engine/tests/fixtures/excel-calc-probes/P14/eng9-20261103/conditions/A1-sweeps-1-zero-reverse/trial-01/input.xlsx"),
            "A1-sweeps-100-asymmetric" => include_bytes!("../../calc-engine/tests/fixtures/excel-calc-probes/P14/eng9-20261103/conditions/A1-sweeps-100-asymmetric/trial-01/input.xlsx"),
            "A1-sweeps-100-asymmetric-Calculate" => include_bytes!("../../calc-engine/tests/fixtures/excel-calc-probes/P14/eng9-20261103/conditions/A1-sweeps-100-asymmetric-Calculate/trial-01/input.xlsx"),
            "A1-sweeps-100-asymmetric-Full" => include_bytes!("../../calc-engine/tests/fixtures/excel-calc-probes/P14/eng9-20261103/conditions/A1-sweeps-100-asymmetric-Full/trial-01/input.xlsx"),
            "A1-sweeps-100-asymmetric-Range" => include_bytes!("../../calc-engine/tests/fixtures/excel-calc-probes/P14/eng9-20261103/conditions/A1-sweeps-100-asymmetric-Range/trial-01/input.xlsx"),
            "A1-sweeps-100-asymmetric-Sheet" => include_bytes!("../../calc-engine/tests/fixtures/excel-calc-probes/P14/eng9-20261103/conditions/A1-sweeps-100-asymmetric-Sheet/trial-01/input.xlsx"),
            "A1-sweeps-100-asymmetric-reverse" => include_bytes!("../../calc-engine/tests/fixtures/excel-calc-probes/P14/eng9-20261103/conditions/A1-sweeps-100-asymmetric-reverse/trial-01/input.xlsx"),
            "A1-sweeps-100-zero" => include_bytes!("../../calc-engine/tests/fixtures/excel-calc-probes/P14/eng9-20261103/conditions/A1-sweeps-100-zero/trial-01/input.xlsx"),
            "A1-sweeps-100-zero-Calculate" => include_bytes!("../../calc-engine/tests/fixtures/excel-calc-probes/P14/eng9-20261103/conditions/A1-sweeps-100-zero-Calculate/trial-01/input.xlsx"),
            "A1-sweeps-100-zero-Full" => include_bytes!("../../calc-engine/tests/fixtures/excel-calc-probes/P14/eng9-20261103/conditions/A1-sweeps-100-zero-Full/trial-01/input.xlsx"),
            "A1-sweeps-100-zero-Range" => include_bytes!("../../calc-engine/tests/fixtures/excel-calc-probes/P14/eng9-20261103/conditions/A1-sweeps-100-zero-Range/trial-01/input.xlsx"),
            "A1-sweeps-100-zero-Sheet" => include_bytes!("../../calc-engine/tests/fixtures/excel-calc-probes/P14/eng9-20261103/conditions/A1-sweeps-100-zero-Sheet/trial-01/input.xlsx"),
            "A1-sweeps-100-zero-reverse" => include_bytes!("../../calc-engine/tests/fixtures/excel-calc-probes/P14/eng9-20261103/conditions/A1-sweeps-100-zero-reverse/trial-01/input.xlsx"),
            "A1-sweeps-2-asymmetric" => include_bytes!("../../calc-engine/tests/fixtures/excel-calc-probes/P14/eng9-20261103/conditions/A1-sweeps-2-asymmetric/trial-01/input.xlsx"),
            "A1-sweeps-2-asymmetric-Calculate" => include_bytes!("../../calc-engine/tests/fixtures/excel-calc-probes/P14/eng9-20261103/conditions/A1-sweeps-2-asymmetric-Calculate/trial-01/input.xlsx"),
            "A1-sweeps-2-asymmetric-Full" => include_bytes!("../../calc-engine/tests/fixtures/excel-calc-probes/P14/eng9-20261103/conditions/A1-sweeps-2-asymmetric-Full/trial-01/input.xlsx"),
            "A1-sweeps-2-asymmetric-Range" => include_bytes!("../../calc-engine/tests/fixtures/excel-calc-probes/P14/eng9-20261103/conditions/A1-sweeps-2-asymmetric-Range/trial-01/input.xlsx"),
            "A1-sweeps-2-asymmetric-Sheet" => include_bytes!("../../calc-engine/tests/fixtures/excel-calc-probes/P14/eng9-20261103/conditions/A1-sweeps-2-asymmetric-Sheet/trial-01/input.xlsx"),
            "A1-sweeps-2-asymmetric-reverse" => include_bytes!("../../calc-engine/tests/fixtures/excel-calc-probes/P14/eng9-20261103/conditions/A1-sweeps-2-asymmetric-reverse/trial-01/input.xlsx"),
            "A1-sweeps-2-zero" => include_bytes!("../../calc-engine/tests/fixtures/excel-calc-probes/P14/eng9-20261103/conditions/A1-sweeps-2-zero/trial-01/input.xlsx"),
            "A1-sweeps-2-zero-Calculate" => include_bytes!("../../calc-engine/tests/fixtures/excel-calc-probes/P14/eng9-20261103/conditions/A1-sweeps-2-zero-Calculate/trial-01/input.xlsx"),
            "A1-sweeps-2-zero-Full" => include_bytes!("../../calc-engine/tests/fixtures/excel-calc-probes/P14/eng9-20261103/conditions/A1-sweeps-2-zero-Full/trial-01/input.xlsx"),
            "A1-sweeps-2-zero-Range" => include_bytes!("../../calc-engine/tests/fixtures/excel-calc-probes/P14/eng9-20261103/conditions/A1-sweeps-2-zero-Range/trial-01/input.xlsx"),
            "A1-sweeps-2-zero-Sheet" => include_bytes!("../../calc-engine/tests/fixtures/excel-calc-probes/P14/eng9-20261103/conditions/A1-sweeps-2-zero-Sheet/trial-01/input.xlsx"),
            "A1-sweeps-2-zero-reverse" => include_bytes!("../../calc-engine/tests/fixtures/excel-calc-probes/P14/eng9-20261103/conditions/A1-sweeps-2-zero-reverse/trial-01/input.xlsx"),
            "A1-sweeps-3-asymmetric" => include_bytes!("../../calc-engine/tests/fixtures/excel-calc-probes/P14/eng9-20261103/conditions/A1-sweeps-3-asymmetric/trial-01/input.xlsx"),
            "A1-sweeps-3-asymmetric-Calculate" => include_bytes!("../../calc-engine/tests/fixtures/excel-calc-probes/P14/eng9-20261103/conditions/A1-sweeps-3-asymmetric-Calculate/trial-01/input.xlsx"),
            "A1-sweeps-3-asymmetric-Full" => include_bytes!("../../calc-engine/tests/fixtures/excel-calc-probes/P14/eng9-20261103/conditions/A1-sweeps-3-asymmetric-Full/trial-01/input.xlsx"),
            "A1-sweeps-3-asymmetric-Range" => include_bytes!("../../calc-engine/tests/fixtures/excel-calc-probes/P14/eng9-20261103/conditions/A1-sweeps-3-asymmetric-Range/trial-01/input.xlsx"),
            "A1-sweeps-3-asymmetric-Sheet" => include_bytes!("../../calc-engine/tests/fixtures/excel-calc-probes/P14/eng9-20261103/conditions/A1-sweeps-3-asymmetric-Sheet/trial-01/input.xlsx"),
            "A1-sweeps-3-asymmetric-reverse" => include_bytes!("../../calc-engine/tests/fixtures/excel-calc-probes/P14/eng9-20261103/conditions/A1-sweeps-3-asymmetric-reverse/trial-01/input.xlsx"),
            "A1-sweeps-3-zero" => include_bytes!("../../calc-engine/tests/fixtures/excel-calc-probes/P14/eng9-20261103/conditions/A1-sweeps-3-zero/trial-01/input.xlsx"),
            "A1-sweeps-3-zero-Calculate" => include_bytes!("../../calc-engine/tests/fixtures/excel-calc-probes/P14/eng9-20261103/conditions/A1-sweeps-3-zero-Calculate/trial-01/input.xlsx"),
            "A1-sweeps-3-zero-Full" => include_bytes!("../../calc-engine/tests/fixtures/excel-calc-probes/P14/eng9-20261103/conditions/A1-sweeps-3-zero-Full/trial-01/input.xlsx"),
            "A1-sweeps-3-zero-Range" => include_bytes!("../../calc-engine/tests/fixtures/excel-calc-probes/P14/eng9-20261103/conditions/A1-sweeps-3-zero-Range/trial-01/input.xlsx"),
            "A1-sweeps-3-zero-Sheet" => include_bytes!("../../calc-engine/tests/fixtures/excel-calc-probes/P14/eng9-20261103/conditions/A1-sweeps-3-zero-Sheet/trial-01/input.xlsx"),
            "A1-sweeps-3-zero-reverse" => include_bytes!("../../calc-engine/tests/fixtures/excel-calc-probes/P14/eng9-20261103/conditions/A1-sweeps-3-zero-reverse/trial-01/input.xlsx"),
            "A1-sweeps-5-asymmetric" => include_bytes!("../../calc-engine/tests/fixtures/excel-calc-probes/P14/eng9-20261103/conditions/A1-sweeps-5-asymmetric/trial-01/input.xlsx"),
            "A1-sweeps-5-asymmetric-Calculate" => include_bytes!("../../calc-engine/tests/fixtures/excel-calc-probes/P14/eng9-20261103/conditions/A1-sweeps-5-asymmetric-Calculate/trial-01/input.xlsx"),
            "A1-sweeps-5-asymmetric-Full" => include_bytes!("../../calc-engine/tests/fixtures/excel-calc-probes/P14/eng9-20261103/conditions/A1-sweeps-5-asymmetric-Full/trial-01/input.xlsx"),
            "A1-sweeps-5-asymmetric-Range" => include_bytes!("../../calc-engine/tests/fixtures/excel-calc-probes/P14/eng9-20261103/conditions/A1-sweeps-5-asymmetric-Range/trial-01/input.xlsx"),
            "A1-sweeps-5-asymmetric-Sheet" => include_bytes!("../../calc-engine/tests/fixtures/excel-calc-probes/P14/eng9-20261103/conditions/A1-sweeps-5-asymmetric-Sheet/trial-01/input.xlsx"),
            "A1-sweeps-5-asymmetric-reverse" => include_bytes!("../../calc-engine/tests/fixtures/excel-calc-probes/P14/eng9-20261103/conditions/A1-sweeps-5-asymmetric-reverse/trial-01/input.xlsx"),
            "A1-sweeps-5-zero" => include_bytes!("../../calc-engine/tests/fixtures/excel-calc-probes/P14/eng9-20261103/conditions/A1-sweeps-5-zero/trial-01/input.xlsx"),
            "A1-sweeps-5-zero-Calculate" => include_bytes!("../../calc-engine/tests/fixtures/excel-calc-probes/P14/eng9-20261103/conditions/A1-sweeps-5-zero-Calculate/trial-01/input.xlsx"),
            "A1-sweeps-5-zero-Full" => include_bytes!("../../calc-engine/tests/fixtures/excel-calc-probes/P14/eng9-20261103/conditions/A1-sweeps-5-zero-Full/trial-01/input.xlsx"),
            "A1-sweeps-5-zero-Range" => include_bytes!("../../calc-engine/tests/fixtures/excel-calc-probes/P14/eng9-20261103/conditions/A1-sweeps-5-zero-Range/trial-01/input.xlsx"),
            "A1-sweeps-5-zero-Sheet" => include_bytes!("../../calc-engine/tests/fixtures/excel-calc-probes/P14/eng9-20261103/conditions/A1-sweeps-5-zero-Sheet/trial-01/input.xlsx"),
            "A1-sweeps-5-zero-reverse" => include_bytes!("../../calc-engine/tests/fixtures/excel-calc-probes/P14/eng9-20261103/conditions/A1-sweeps-5-zero-reverse/trial-01/input.xlsx"),
            "A2-eighth-0000000000000000" => include_bytes!("../../calc-engine/tests/fixtures/excel-calc-probes/P14/eng9-20261103/conditions/A2-eighth-0000000000000000/trial-01/input.xlsx"),
            "A2-eighth-3fbfffffffffffff" => include_bytes!("../../calc-engine/tests/fixtures/excel-calc-probes/P14/eng9-20261103/conditions/A2-eighth-3fbfffffffffffff/trial-01/input.xlsx"),
            "A2-eighth-3fc0000000000000" => include_bytes!("../../calc-engine/tests/fixtures/excel-calc-probes/P14/eng9-20261103/conditions/A2-eighth-3fc0000000000000/trial-01/input.xlsx"),
            "A2-eighth-3fc0000000000001" => include_bytes!("../../calc-engine/tests/fixtures/excel-calc-probes/P14/eng9-20261103/conditions/A2-eighth-3fc0000000000001/trial-01/input.xlsx"),
            "A2-half-0000000000000000" => include_bytes!("../../calc-engine/tests/fixtures/excel-calc-probes/P14/eng9-20261103/conditions/A2-half-0000000000000000/trial-01/input.xlsx"),
            "A2-half-3fbfffffffffffff" => include_bytes!("../../calc-engine/tests/fixtures/excel-calc-probes/P14/eng9-20261103/conditions/A2-half-3fbfffffffffffff/trial-01/input.xlsx"),
            "A2-half-3fc0000000000000" => include_bytes!("../../calc-engine/tests/fixtures/excel-calc-probes/P14/eng9-20261103/conditions/A2-half-3fc0000000000000/trial-01/input.xlsx"),
            "A2-half-3fc0000000000001" => include_bytes!("../../calc-engine/tests/fixtures/excel-calc-probes/P14/eng9-20261103/conditions/A2-half-3fc0000000000001/trial-01/input.xlsx"),
            "A3-both-alone" => include_bytes!("../../calc-engine/tests/fixtures/excel-calc-probes/P14/eng9-20261103/conditions/A3-both-alone/trial-01/input.xlsx"),
            "A3-both-amplifiers" => include_bytes!("../../calc-engine/tests/fixtures/excel-calc-probes/P14/eng9-20261103/conditions/A3-both-amplifiers/trial-01/input.xlsx"),
            "A3-eighth-alone" => include_bytes!("../../calc-engine/tests/fixtures/excel-calc-probes/P14/eng9-20261103/conditions/A3-eighth-alone/trial-01/input.xlsx"),
            "A3-eighth-amplifier" => include_bytes!("../../calc-engine/tests/fixtures/excel-calc-probes/P14/eng9-20261103/conditions/A3-eighth-amplifier/trial-01/input.xlsx"),
            "A3-half-alone" => include_bytes!("../../calc-engine/tests/fixtures/excel-calc-probes/P14/eng9-20261103/conditions/A3-half-alone/trial-01/input.xlsx"),
            "A3-half-amplifier" => include_bytes!("../../calc-engine/tests/fixtures/excel-calc-probes/P14/eng9-20261103/conditions/A3-half-amplifier/trial-01/input.xlsx"),
            "A4-pure" => include_bytes!("../../calc-engine/tests/fixtures/excel-calc-probes/P14/eng9-20261103/conditions/A4-pure/trial-01/input.xlsx"),
            "B6-NA" => include_bytes!("../../calc-engine/tests/fixtures/excel-calc-probes/P14/eng9-20261103/conditions/B6-NA/trial-01/input.xlsx"),
            "B6-boolean" => include_bytes!("../../calc-engine/tests/fixtures/excel-calc-probes/P14/eng9-20261103/conditions/B6-boolean/trial-01/input.xlsx"),
            "B6-changing-error" => include_bytes!("../../calc-engine/tests/fixtures/excel-calc-probes/P14/eng9-20261103/conditions/B6-changing-error/trial-01/input.xlsx"),
            "B6-number-to-text" => include_bytes!("../../calc-engine/tests/fixtures/excel-calc-probes/P14/eng9-20261103/conditions/B6-number-to-text/trial-01/input.xlsx"),
            "B6-stable-text" => include_bytes!("../../calc-engine/tests/fixtures/excel-calc-probes/P14/eng9-20261103/conditions/B6-stable-text/trial-01/input.xlsx"),
            "B6-toggle-text" => include_bytes!("../../calc-engine/tests/fixtures/excel-calc-probes/P14/eng9-20261103/conditions/B6-toggle-text/trial-01/input.xlsx"),
            "B6-untaken-self" => include_bytes!("../../calc-engine/tests/fixtures/excel-calc-probes/P14/eng9-20261103/conditions/B6-untaken-self/trial-01/input.xlsx"),
            "B7-indirect" => include_bytes!("../../calc-engine/tests/fixtures/excel-calc-probes/P14/eng9-20261103/conditions/B7-indirect/trial-01/input.xlsx"),
            "B7-static" => include_bytes!("../../calc-engine/tests/fixtures/excel-calc-probes/P14/eng9-20261103/conditions/B7-static/trial-01/input.xlsx"),
            "control-dc_10y_iteration" => include_bytes!("../../calc-engine/tests/fixtures/excel-calc-probes/P14/eng9-20261103/conditions/control-dc_10y_iteration/trial-01/input.xlsx"),
            "control-dc_86f" => include_bytes!("../../calc-engine/tests/fixtures/excel-calc-probes/P14/eng9-20261103/conditions/control-dc_86f/trial-01/input.xlsx"),
            _ => panic!("missing input workbook for {name}"),
        };
        assert_eq!(
            calc_verdict::sha256_hex(bytes),
            result["input_sha256"].as_str().unwrap(),
            "{name}"
        );
        let stages = |name: &str| observations.iter().find(|o| o["stage"] == name).unwrap();
        let seeds = stages("seeds-verified")["cells"].as_array().unwrap();
        for budget in [usize::MAX, 1] {
            let mut book = session_without_recalc(bytes);
            assert!(
                book.dispatch(SkinIntent::SetCalcMode(CalcMode::Manual))
                    .accepted()
            );
            book.set_calculation_budget(WorkBudget::evaluations(budget));
            let cells = seeds
                .iter()
                .map(|cell| {
                    let sheet = book
                        .host()
                        .sheets()
                        .iter()
                        .find(|sheet| {
                            sheet.name().text().to_string_lossy() == cell["sheet"].as_str().unwrap()
                        })
                        .unwrap()
                        .id();
                    CellKey::new(
                        sheet,
                        cell["row"].as_u64().unwrap() as u32,
                        cell["column"].as_u64().unwrap() as u32,
                    )
                    .unwrap()
                })
                .collect::<Vec<_>>();
            let request = ProjectionRequest { cells };
            let initial = project(book.host(), &request).unwrap();
            for (cell, expected) in initial.cells().iter().zip(seeds) {
                let ProjectedValue::FileCached(cache) = cell.value() else {
                    panic!(
                        "{name}: initial value must remain a file cache: {:?}",
                        cell.value()
                    );
                };
                assert_eq!(
                    cache.value().to_value(),
                    p14_value(&expected["value"]),
                    "{name}"
                );
            }
            for command in 1..=condition["commands"].as_u64().unwrap() {
                let intent = match condition["operation"].as_str().unwrap() {
                    "Rebuild" => SkinIntent::Rebuild,
                    "Calculate" => SkinIntent::Recalculate,
                    "Full" => SkinIntent::Calculate {
                        scope: calc_engine::CalcScope::Workbook,
                        selection: calc_engine::EvalSelection::ForceAll,
                    },
                    "Sheet" => SkinIntent::Calculate {
                        scope: calc_engine::CalcScope::Sheet(request.cells[0].sheet()),
                        selection: calc_engine::EvalSelection::ForceAll,
                    },
                    "Range" => SkinIntent::CalculateRange {
                        areas: vec![GridArea::new(request.cells[0], request.cells[3]).unwrap()],
                    },
                    other => panic!("unhandled operation {other}"),
                };
                let receipt = book.dispatch(intent);
                assert!(receipt.accepted(), "{name}: {receipt:?}");
                // Admission can mark a previous result stale; subsequent owner
                // yields retain that admitted view until terminal publication.
                let before = project(book.host(), &request).unwrap();
                if condition["operation"] == "Range" {
                    // These cells remain uncalculated; a held seed never scores
                    // as a match to the recorded no-op observation.
                    p14_advance(&mut book);
                    let after = project(book.host(), &request).unwrap();
                    assert!(after.recalc_failure().is_none(), "{name}: {:?}", after.recalc_failure());
                    let events = book.take_calculation_events();
                    let done = events.iter().rev().find_map(|e| match e {
                        calc_host::CalcEvent::Completed(r) => Some(r), _ => None,
                    }).unwrap();
                    assert_eq!(done.outcome, calc_host::CompletionOutcome::Incomplete);
                    assert_eq!(done.coverage.calculated, 0);
                    assert_eq!(done.coverage.required, request.cells.len());
                    assert_eq!(done.coverage.refused, request.cells.len());
                    assert_eq!(p14_refused(&events, done.command), request.cells.iter().copied().collect());
                    assert!(!book.has_pending_calculation());
                    for (cell, seed) in after.cells().iter().zip(seeds) {
                        let cached = match cell.value() {
                            ProjectedValue::FileCached(cache) => cache,
                            ProjectedValue::Stale(stale) => stale.file_cache().unwrap(),
                            other => panic!("{name}: a refusal must not calculate {other:?}"),
                        };
                        assert_eq!(cached.value().to_value(), p14_value(&seed["value"]));
                        refused_values += 1;
                    }
                    continue;
                }
                let mut steps = 0;
                while book.has_pending_calculation() {
                    // DNA publication policy, not a claim about unrecorded Excel observers.
                    assert_eq!(
                        project(book.host(), &request).unwrap().cells(),
                        before.cells(),
                        "{name}"
                    );
                    let progress = book.advance_calculation();
                    assert!(
                        !matches!(progress, HostProgress::Failed(_)),
                        "{name}: {progress:?}"
                    );
                    steps += 1;
                    assert!(steps < 10000, "{name}: finite work must finish");
                }
                let actual = project(book.host(), &request).unwrap();
                let expected = stages(&format!("after-command-{command}"))["cells"]
                    .as_array()
                    .unwrap();
                assert_eq!(actual.cells().len(), expected.len());
                for (cell, expected) in actual.cells().iter().zip(expected) {
                    comparisons += 1;
                    let wanted = p14_value(&expected["value"]);
                    match cell.value() {
                        ProjectedValue::Calculated { value, calculation }
                            if value == &wanted && !calculation.held_by_circular_rule() => {}
                        other => failures.push(format!("{name}, budget {budget}, command {command}, {:?}: expected {wanted:?}, got {other:?}", cell.at())),
                    }
                }
                commands += 1;
            }
        }
    }
    assert_eq!(refused_range.len(), 10);
    assert_eq!(refused_values, 160);
    assert_eq!(commands, 300);
    assert_eq!(comparisons, 1014);
    assert!(
        failures.is_empty(),
        "{} recorded comparisons failed:\n{}",
        failures.len(),
        failures.join("\n")
    );
}

// Original P14 inputs and result objects. Keep one typed-value decoder and host route.
fn p14_input(name: &str) -> &'static [u8] {
    match name {
        "eng17-logical-baseline-false-negative-eighth-half" => include_bytes!("../../calc-engine/tests/fixtures/excel-calc-probes/P14/eng17-20261105/inputs/eng17-logical-baseline-false-negative-eighth-half.xlsx"),
        "eng17-logical-baseline-false-positive-eighth-half" => include_bytes!("../../calc-engine/tests/fixtures/excel-calc-probes/P14/eng17-20261105/inputs/eng17-logical-baseline-false-positive-eighth-half.xlsx"),
        "eng17-logical-baseline-true-negative-eighth-half" => include_bytes!("../../calc-engine/tests/fixtures/excel-calc-probes/P14/eng17-20261105/inputs/eng17-logical-baseline-true-negative-eighth-half.xlsx"),
        "eng17-logical-baseline-true-negative-eighth-one" => include_bytes!("../../calc-engine/tests/fixtures/excel-calc-probes/P14/eng17-20261105/inputs/eng17-logical-baseline-true-negative-eighth-one.xlsx"),
        "eng17-logical-baseline-true-negative-one-half" => include_bytes!("../../calc-engine/tests/fixtures/excel-calc-probes/P14/eng17-20261105/inputs/eng17-logical-baseline-true-negative-one-half.xlsx"),
        "eng17-logical-baseline-true-positive-eighth-half" => include_bytes!("../../calc-engine/tests/fixtures/excel-calc-probes/P14/eng17-20261105/inputs/eng17-logical-baseline-true-positive-eighth-half.xlsx"),
        "eng17-logical-baseline-true-positive-eighth-one" => include_bytes!("../../calc-engine/tests/fixtures/excel-calc-probes/P14/eng17-20261105/inputs/eng17-logical-baseline-true-positive-eighth-one.xlsx"),
        "eng17-logical-baseline-true-positive-one-three-halves" => include_bytes!("../../calc-engine/tests/fixtures/excel-calc-probes/P14/eng17-20261105/inputs/eng17-logical-baseline-true-positive-one-three-halves.xlsx"),
        "eng14-logical-number-baseline-false" => include_bytes!("../../calc-engine/tests/fixtures/excel-calc-probes/P14/eng14-20261104/inputs/eng14-logical-number-baseline-false.xlsx"),
        "eng14-logical-number-baseline-one" => include_bytes!("../../calc-engine/tests/fixtures/excel-calc-probes/P14/eng14-20261104/inputs/eng14-logical-number-baseline-one.xlsx"),
        "eng14-logical-number-baseline-true" => include_bytes!("../../calc-engine/tests/fixtures/excel-calc-probes/P14/eng14-20261104/inputs/eng14-logical-number-baseline-true.xlsx"),
        "eng14-logical-only-false-n2" => include_bytes!("../../calc-engine/tests/fixtures/excel-calc-probes/P14/eng14-20261104/inputs/eng14-logical-only-false-n2.xlsx"),
        "eng14-logical-only-false-n3" => include_bytes!("../../calc-engine/tests/fixtures/excel-calc-probes/P14/eng14-20261104/inputs/eng14-logical-only-false-n3.xlsx"),
        "eng14-logical-only-true-n2" => include_bytes!("../../calc-engine/tests/fixtures/excel-calc-probes/P14/eng14-20261104/inputs/eng14-logical-only-true-n2.xlsx"),
        "eng14-logical-only-true-n3" => include_bytes!("../../calc-engine/tests/fixtures/excel-calc-probes/P14/eng14-20261104/inputs/eng14-logical-only-true-n3.xlsx"),
        "eng14-range-cyclic-shape-reader" => include_bytes!("../../calc-engine/tests/fixtures/excel-calc-probes/P14/eng14-20261104/inputs/eng14-range-cyclic-shape-reader.xlsx"),
        "eng14-range-dynamic-cycle" => include_bytes!("../../calc-engine/tests/fixtures/excel-calc-probes/P14/eng14-20261104/inputs/eng14-range-dynamic-cycle.xlsx"),
        "eng14-range-dynamic-cycle-reader" => include_bytes!("../../calc-engine/tests/fixtures/excel-calc-probes/P14/eng14-20261104/inputs/eng14-range-dynamic-cycle-reader.xlsx"),
        "eng14-range-hidden-self-reader" => include_bytes!("../../calc-engine/tests/fixtures/excel-calc-probes/P14/eng14-20261104/inputs/eng14-range-hidden-self-reader.xlsx"),
        "eng14-range-untaken-cycle-reader" => include_bytes!("../../calc-engine/tests/fixtures/excel-calc-probes/P14/eng14-20261104/inputs/eng14-range-untaken-cycle-reader.xlsx"),
        "eng14-rebuild-hidden-self-reader" => include_bytes!("../../calc-engine/tests/fixtures/excel-calc-probes/P14/eng14-20261104/inputs/eng14-rebuild-hidden-self-reader.xlsx"),
        "eng14-text-error-only-n2" => include_bytes!("../../calc-engine/tests/fixtures/excel-calc-probes/P14/eng14-20261104/inputs/eng14-text-error-only-n2.xlsx"),
        "eng14-text-error-only-n5" => include_bytes!("../../calc-engine/tests/fixtures/excel-calc-probes/P14/eng14-20261104/inputs/eng14-text-error-only-n5.xlsx"),
        _ => panic!("unknown P14 input {name}"),
    }
}

/// Six unchanged pure Range/Rebuild conditions. Cache equality remains a nonpass;
/// a shape result with imported ancestry also remains stale, despite its own evaluation.
#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn dc_hsa5_11__eng14_range_paths_and_retained_seeds() {
    use calc_host::{CalcEvent, CompletionOutcome, WorkBudget};
    use calc_host::value::GridArea;
    use serde_json::Value;
    let records = std::str::from_utf8(include_bytes!(
        "../../calc-engine/tests/fixtures/excel-calc-probes/P14/eng14-20261104/range-observations.jsonl"
    )).unwrap().lines().map(|line| serde_json::from_str::<Value>(line).unwrap()).collect::<Vec<_>>();
    assert_eq!(records.len(), 6);
    let (mut evaluated, mut legacy_values, mut stale_evaluated, mut retained) = (0, 0, 0, 0);
    for record in records {
        let name = record["condition"].as_str().unwrap();
        let result = &record["result"];
        let stage = |label: &str| result["observations"].as_array().unwrap().iter().find(|s| s["stage"] == label).unwrap();
        let seeds = stage("seeds-verified")["cells"].as_array().unwrap();
        for budget in [usize::MAX, 1] {
            let mut book = HostSession::open(Arc::new(MemorySource::new(p14_input(name).to_vec())), &OpenPolicy::default(),
                HostOptions { calc_mode: Some(CalcMode::Manual), ..HostOptions::default() }).unwrap();
            book.set_calculation_budget(WorkBudget::evaluations(budget));
            let settings = book.host().model().definitions().iteration();
            assert!(settings.enabled());
            assert_eq!(u64::from(settings.max_iterations()), result["condition"]["iterations"].as_u64().unwrap());
            assert_eq!(settings.max_change().to_bits(), u64::from_str_radix(result["condition"]["change_bits"].as_str().unwrap(),16).unwrap());
            let sheet = book.host().sheets()[0].id();
            let at = |column| CellKey::new(sheet, 1, column).unwrap();
            let request = ProjectionRequest { cells: seeds.iter().map(|s| at(s["column"].as_u64().unwrap() as u32)).collect() };
            let initial = project(book.host(), &request).unwrap();
            let sources = initial.cells().iter().zip(seeds).map(|(cell, seed)| {
                let ProjectedValue::FileCached(cache) = cell.value() else { panic!("{name}: seed must be imported"); };
                assert_eq!(cache.value().to_value(), p14_value(&seed["value"]));
                cache.source().clone()
            }).collect::<Vec<_>>();
            for command in 1..=2 {
                book.take_calculation_events();
                let rebuild = result["condition"]["operation"] == "Rebuild";
                let intent = if rebuild { SkinIntent::Rebuild } else {
                    SkinIntent::CalculateRange { areas: vec![GridArea::new(at(1), at(4)).unwrap()] }
                };
                assert!(book.dispatch(intent).accepted());
                p14_advance(&mut book);
                let page = project(book.host(), &request).unwrap();
                let dynamic_cycle = name == "eng14-range-dynamic-cycle";
                assert!(page.recalc_failure().is_none(), "{name}: {:?}", page.recalc_failure());
                if dynamic_cycle {
                    assert_eq!(book.host().circular_cells(), vec![at(1), at(2)],
                        "an excluded reader is not a proven cyclic producer");
                }
                let expected = stage(&format!("after-command-{command}"))["cells"].as_array().unwrap();
                let events = book.take_calculation_events();
                let producing_command = events.iter().find_map(|e| match e {
                    CalcEvent::Completed(receipt) => Some(receipt.command), _ => None,
                });
                {
                    let done = events.iter().find_map(|e| match e { CalcEvent::Completed(r) => Some(r), _ => None }).unwrap();
                    assert_eq!(done.coverage.required, seeds.len());
                    let calculated = if rebuild || name.ends_with("untaken-cycle-reader") || name.ends_with("cyclic-shape-reader") { 2 } else if dynamic_cycle || name.ends_with("dynamic-cycle-reader") { 1 } else { 0 };
                    assert_eq!(done.coverage.calculated, calculated);
                    assert_eq!(done.coverage.refused, seeds.len() - calculated);
                    assert_eq!(done.outcome, CompletionOutcome::Incomplete);
                    if rebuild { assert_eq!(done.coverage.uncertified, 2); }
                    if name.ends_with("cyclic-shape-reader") {
                        assert_eq!(done.coverage.boundary_cached, 1);
                        assert_eq!(done.coverage.imported_cached, 1);
                    }
                    let mut union = std::collections::BTreeSet::new();
                    for e in &events { if let CalcEvent::TargetsRefused { targets, owner, .. } = e {
                        assert_eq!(*owner, "dc-hsa5.11");
                        for target in targets { assert!(union.insert(*target), "no duplicate refusal {target:?}"); }
                    }}
                    assert_eq!(union.len(), done.coverage.refused);
                }
                for (index, (cell, expected)) in page.cells().iter().zip(expected).enumerate() {
                    let column = cell.at().column();
                    let wanted = p14_value(&expected["value"]);
                    let fresh = rebuild || (column == 4 && name != "eng14-range-hidden-self-reader")
                        || (column == 3 && name == "eng14-range-untaken-cycle-reader");
                    if fresh {
                        // Manual Range retains demand: its new result may project stale.
                        // Check the producing command, never count display equality as proof.
                        let (value, calculation) = p14_visible(&book, 1, column);
                        assert_eq!(value, wanted, "{name} {command} {column}");
                        let calculation = calculation.expect("the command must evaluate this target");
                        assert!(!calculation.held_by_circular_rule());
                        assert_eq!(calculation.tick().get(), producing_command.unwrap().sequence);
                        if rebuild {
                            // Existing iterative sessions expose final values but lack
                            // SliceV1 invocation certificates; do not invent that proof.
                            if let Some(stamp) = calculation.stamp() {
                                assert!(matches!(stamp.computed_by, calc_engine::contract::ComputationOrigin::Evaluated { .. }));
                            }
                            legacy_values += 1;
                        } else {
                            let stamp = calculation.stamp().unwrap();
                            assert!(matches!(stamp.computed_by, calc_engine::contract::ComputationOrigin::Evaluated { .. }));
                            assert_eq!(stamp.certification.command, producing_command.unwrap());
                            assert!(!stamp.reads.uses_imported_cache && !stamp.reads.uses_boundary_cache);
                            evaluated += 1;
                        }
                    } else if column == 3 && name == "eng14-range-cyclic-shape-reader" {
                        let ProjectedValue::Stale(held) = cell.value() else { panic!("imported shape must not prove freshness: {cell:?}"); };
                        assert_eq!(held.last_value(), Some(&wanted));
                        let stamp = held.last_calculation().unwrap().stamp().unwrap();
                        assert!(matches!(stamp.computed_by, calc_engine::contract::ComputationOrigin::Evaluated { .. }));
                        assert_eq!(stamp.certification.command, producing_command.unwrap());
                        assert!(stamp.reads.uses_boundary_cache && stamp.reads.uses_imported_cache);
                        let span = stamp.reads.boundary_caches.iter().find(|span| span.region == GridArea::cell(at(1))).unwrap();
                        assert_eq!((span.values, span.dirty, span.imported), (1, 1, 1));
                        assert!(span.imported_sources.contains(&sources[0]));
                        assert!(stamp.reads.dependencies.iter().any(|read| read.request.purpose == calc_fml::ReadPurpose::ReferenceShape && read.request.facets == calc_fml::ObservableFacets::Shape && read.region == GridArea::cell(at(1))));
                        stale_evaluated += 1; // Explicit nonpass, even though displayed value matches.
                    } else {
                        let shown = p14_visible(&book, 1, column);
                        assert_eq!(shown.0, p14_value(&seeds[index]["value"]), "{name}: retain seed");
                        assert!(shown.1.is_none(), "retained imported seed must not acquire calculation identity");
                        assert_eq!(shown.0, wanted);
                        retained += 1;
                    }
                }
            }
        }
    }
    assert_eq!((evaluated, legacy_values, stale_evaluated, retained), (20, 8, 4, 48));
}

/// All nine original stopping controls, including the two earlier logical-TRUE
/// mismatches. Expected values remain the original recorded observations.
#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn dc_hsa5_11__eng14_supported_nonnumeric_stopping() {
    use calc_host::WorkBudget;
    use serde_json::Value;
    let records = std::str::from_utf8(include_bytes!(
        "../../calc-engine/tests/fixtures/excel-calc-probes/P14/eng14-20261104/stopping-observations.jsonl"
    )).unwrap().lines().map(|line| serde_json::from_str::<Value>(line).unwrap()).collect::<Vec<_>>();
    assert_eq!(records.len(), 9);
    let mut checked = 0;
    for record in records {
        let name = record["condition"].as_str().unwrap();
        let result = &record["result"];
        for budget in [usize::MAX, 1] {
            let mut book = HostSession::open(Arc::new(MemorySource::new(p14_input(name).to_vec())), &OpenPolicy::default(),
                HostOptions { calc_mode: Some(CalcMode::Manual), ..HostOptions::default() }).unwrap();
            book.set_calculation_budget(WorkBudget::evaluations(budget));
            for command in 1..=2 {
                assert!(book.dispatch(SkinIntent::Rebuild).accepted());
                p14_advance(&mut book);
                let expected = result["observations"].as_array().unwrap().iter().find(|s| s["stage"] == format!("after-command-{command}")).unwrap();
                let at = CellKey::new(book.host().sheets()[0].id(), 1, 1).unwrap();
                let page = project(book.host(), &ProjectionRequest { cells: vec![at] }).unwrap();
                let ProjectedValue::Calculated { value, calculation } = page.cells()[0].value() else { panic!("{name}: {:?}", page.cells()[0]); };
                assert_eq!(value, &p14_value(&expected["cells"][0]["value"]), "{name} {command}");
                if let Some(stamp) = calculation.stamp() {
                    assert!(matches!(stamp.computed_by, calc_engine::contract::ComputationOrigin::Evaluated { .. }));
                }
                assert!(!calculation.held_by_circular_rule());
                checked += 1;
            }
        }
    }
    assert_eq!(checked, 36);
}

/// Eight original conditions, each repeated ten times in fresh Excel processes.
/// Final values establish the stopping boundary, not an internal sweep count.
#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn dc_hsa5_14__eng17_logical_seed_stopping_baseline() {
    use calc_host::WorkBudget;
    use serde_json::Value;
    let records = std::str::from_utf8(include_bytes!(
        "../../calc-engine/tests/fixtures/excel-calc-probes/P14/eng17-20261105/observations.jsonl"
    )).unwrap().lines().map(|line| serde_json::from_str::<Value>(line).unwrap()).collect::<Vec<_>>();
    assert_eq!(records.len(), 8);
    let mut checked = 0;
    let mut mismatches = Vec::new();
    for record in records {
        let name = record["condition"].as_str().unwrap();
        let bytes = p14_input(name);
        assert_eq!(calc_verdict::sha256_hex(bytes), record["input_sha256"].as_str().unwrap());
        let stages = record["result"]["observations"].as_array().unwrap();
        let stage = |name: &str| stages.iter().find(|s| s["stage"] == name).unwrap();
        for budget in [usize::MAX, 1] {
            let mut book = HostSession::open(Arc::new(MemorySource::new(bytes.to_vec())),
                &OpenPolicy::default(), HostOptions { calc_mode: Some(CalcMode::Manual), ..HostOptions::default() }).unwrap();
            book.set_calculation_budget(WorkBudget::evaluations(budget));
            let at = CellKey::new(book.host().sheets()[0].id(), 1, 1).unwrap();
            let initial = project(book.host(), &ProjectionRequest { cells: vec![at] }).unwrap();
            let ProjectedValue::FileCached(seed) = initial.cells()[0].value() else { panic!("original logical seed"); };
            assert_eq!(seed.value().to_value(), p14_value(&stage("seeds-verified")["cells"][0]["value"]));
            for command in 1..=2 {
                assert!(book.dispatch(SkinIntent::Rebuild).accepted());
                p14_advance(&mut book);
                let page = project(book.host(), &ProjectionRequest { cells: vec![at] }).unwrap();
                let ProjectedValue::Calculated { value, calculation } = page.cells()[0].value() else { panic!("{name}: {:?}", page.cells()[0]); };
                assert!(!calculation.held_by_circular_rule());
                assert!(!calculation.reads_external_links());
                let expected = p14_value(&stage(&format!("after-command-{command}"))["cells"][0]["value"]);
                if value != &expected {
                    mismatches.push(format!("{name} budget={budget} command={command}: {value:?}, Excel {expected:?}"));
                }
                checked += 1;
            }
        }
    }
    assert_eq!(checked, 32);
    println!("P14 eng17 stopping: {} matched, {} mismatched, {checked} calculated comparisons; no held-cache matches", checked - mismatches.len(), mismatches.len());
    assert!(mismatches.is_empty(), "{}", mismatches.join("\n"));
}

#[derive(Clone, Debug, PartialEq)]
struct P14CallbackEntry {
    ident: i32,
    argument: Option<CalcValue>,
    reads: Vec<CalcValue>,
}

#[derive(Debug)]
struct P14Callback {
    pair: bool,
    sheet: SheetId,
    entries: std::rc::Rc<std::cell::RefCell<Vec<P14CallbackEntry>>>,
    pause: Option<std::rc::Rc<std::cell::Cell<bool>>>,
    independent: Option<CellKey>,
}
impl calc_fml::UserFunction for P14Callback {
    fn cached_reads(&self) -> bool { true }
    fn state_lane(&self) -> Option<u64> {
        Some(std::rc::Rc::as_ptr(&self.entries) as usize as u64)
    }
    fn reevaluation_policy(&self) -> Option<calc_fml::UserReevaluationReason> {
        Some(calc_fml::UserReevaluationReason::ProvisionalArguments)
    }
    fn call(&self, _: &[CalcValue]) -> Result<CalcValue, String> {
        panic!("P14 callbacks require the application's admitted read context")
    }
    fn call_with_context(&self, args: &[CalcValue], context: &mut dyn calc_fml::UserFunctionContext)
        -> Result<CalcValue, String>
    {
        assert_eq!(args.len(), 2);
        let references = if self.pair {
            args.iter().map(|arg| arg.as_reference().expect("recorded Variant formals retain Range objects").clone()).collect::<Vec<_>>()
        } else {
            [1, 2].map(|column| calc_value::ReferenceLike::cell(CellKey::new(self.sheet, 1, column).unwrap())).to_vec()
        };
        let mut reads = Vec::new();
        for _ in 0..if self.pair { 2 } else { 1 } {
            for reference in &references { reads.push(context.values_of(reference)?); }
        }
        if let Some(at) = self.independent {
            reads.push(context.values_of(&calc_value::ReferenceLike::cell(at))?);
        }
        self.entries.borrow_mut().push(P14CallbackEntry {
            ident: if self.pair { 0 } else { args[0].as_number().unwrap() as i32 },
            argument: (!self.pair).then(|| args[1].clone()), reads,
        });
        // These are the recorded callback bodies: PacketStep returns nextValue;
        // PacketPairVariant observes twice and returns its authored constant zero.
        Ok(if self.pair { CalcValue::number(0.0) } else { args[1].clone() })
    }
    fn begin_call(&self, args: &[CalcValue], _: calc_fml::UserInvocationFrame,
        context: &mut dyn calc_fml::UserFunctionContext)
        -> Result<Option<Box<dyn calc_fml::UserFunctionInvocation>>, String>
    {
        let Some(release) = self.pause.as_ref() else { return Ok(None); };
        assert!(self.pair);
        let references = args.iter().map(|arg| arg.as_reference().unwrap().clone()).collect::<Vec<_>>();
        let reads = references.iter().map(|reference| context.values_of(reference)).collect::<Result<Vec<_>, _>>()?;
        let index = self.entries.borrow().len();
        self.entries.borrow_mut().push(P14CallbackEntry { ident: 0, argument: None, reads });
        Ok(Some(Box::new(P14PausedCallback {
            release: release.clone(), entries: self.entries.clone(), index, references,
        })))
    }
}

#[derive(Debug)]
struct P14PausedCallback {
    release: std::rc::Rc<std::cell::Cell<bool>>,
    entries: std::rc::Rc<std::cell::RefCell<Vec<P14CallbackEntry>>>,
    index: usize,
    references: Vec<calc_value::ReferenceLike>,
}
impl calc_fml::UserFunctionInvocation for P14PausedCallback {
    fn resume(&mut self, context: &mut dyn calc_fml::UserFunctionContext, _: u64)
        -> Result<calc_fml::UserFunctionStep, String>
    {
        if !self.release.get() { return Ok(calc_fml::UserFunctionStep::Suspended); }
        for reference in &self.references {
            self.entries.borrow_mut()[self.index].reads.push(context.values_of(reference)?);
        }
        Ok(calc_fml::UserFunctionStep::Complete(CalcValue::number(0.0)))
    }
}

fn p14_trace_value(payload: &str, label: &str) -> CalcValue {
    let text = payload.split_once(label).expect("recorded typed field").1;
    if text.starts_with("vt=0;type=Empty;") { return CalcValue::empty(); }
    assert!(text.starts_with("vt=5;type=Double;"), "{label}: {payload}");
    let bits = text.split_once(";bits=").expect("recorded Double bits").1.split(';').next().unwrap();
    CalcValue::number(f64::from_bits(u64::from_str_radix(bits, 16).unwrap()))
}

fn p14_callback_entries(events: &[serde_json::Value]) -> Vec<P14CallbackEntry> {
    let mut entries: Vec<P14CallbackEntry> = Vec::new();
    for event in events {
        let payload = event["payload"].as_str().unwrap();
        match event["kind"].as_str().unwrap() {
            "entry" => {
                let pair = payload.starts_with("PacketPairVariant;");
                if pair {
                    assert!(payload.contains("left-raw=vt=9;type=Range;"));
                    assert!(payload.contains("right-raw=vt=9;type=Range;"));
                }
                entries.push(P14CallbackEntry {
                    ident: if pair { 0 } else { payload.split_once(";id=").unwrap().1.split(';').next().unwrap().parse().unwrap() },
                    argument: (!pair).then(|| p14_trace_value(payload, ";argument=")), reads: Vec::new(),
                });
            }
            "read" => entries.last_mut().unwrap().reads.extend([
                p14_trace_value(payload, ";left="), p14_trace_value(payload, ";right="),
            ]),
            "exit" => {},
            other => panic!("unexpected measured UDF event {other}"),
        }
    }
    entries
}

fn eng16_input(name: &str) -> &'static [u8] {
    match name {
        "eng16-pair_variant-Range-A1-B1" => include_bytes!("../../calc-engine/tests/fixtures/excel-calc-probes/P14/eng16-20261105/inputs/eng16-pair_variant-Range-A1-B1.xlsm"),
        "eng16-pair_variant-Range-A1-D1" => include_bytes!("../../calc-engine/tests/fixtures/excel-calc-probes/P14/eng16-20261105/inputs/eng16-pair_variant-Range-A1-D1.xlsm"),
        "eng16-pair_variant-Full" => include_bytes!("../../calc-engine/tests/fixtures/excel-calc-probes/P14/eng16-20261105/inputs/eng16-pair_variant-Full.xlsm"),
        "eng16-pair_variant-Rebuild" => include_bytes!("../../calc-engine/tests/fixtures/excel-calc-probes/P14/eng16-20261105/inputs/eng16-pair_variant-Rebuild.xlsm"),
        "eng16-coupled_observed-Full" => include_bytes!("../../calc-engine/tests/fixtures/excel-calc-probes/P14/eng16-20261105/inputs/eng16-coupled_observed-Full.xlsm"),
        "eng16-coupled_observed-Rebuild" => include_bytes!("../../calc-engine/tests/fixtures/excel-calc-probes/P14/eng16-20261105/inputs/eng16-coupled_observed-Rebuild.xlsm"),
        "eng16-coupled_observed_peer-Full" => include_bytes!("../../calc-engine/tests/fixtures/excel-calc-probes/P14/eng16-20261105/inputs/eng16-coupled_observed_peer-Full.xlsm"),
        "eng16-coupled_observed_peer-Rebuild" => include_bytes!("../../calc-engine/tests/fixtures/excel-calc-probes/P14/eng16-20261105/inputs/eng16-coupled_observed_peer-Rebuild.xlsm"),
        "eng16-coupled_pure-Full" => include_bytes!("../../calc-engine/tests/fixtures/excel-calc-probes/P14/eng16-20261105/inputs/eng16-coupled_pure-Full.xlsm"),
        "eng16-coupled_pure-Rebuild" => include_bytes!("../../calc-engine/tests/fixtures/excel-calc-probes/P14/eng16-20261105/inputs/eng16-coupled_pure-Rebuild.xlsm"),
        "eng16-coupled_pure_peer-Full" => include_bytes!("../../calc-engine/tests/fixtures/excel-calc-probes/P14/eng16-20261105/inputs/eng16-coupled_pure_peer-Full.xlsm"),
        "eng16-coupled_pure_peer-Rebuild" => include_bytes!("../../calc-engine/tests/fixtures/excel-calc-probes/P14/eng16-20261105/inputs/eng16-coupled_pure_peer-Rebuild.xlsm"),
        "eng16-coupled_observed_peer-Range-A1-B1" => include_bytes!("../../calc-engine/tests/fixtures/excel-calc-probes/P14/eng16-20261105/inputs/eng16-coupled_observed_peer-Range-A1-B1.xlsm"),
        "eng16-coupled_observed_peer-Range-A1-D1" => include_bytes!("../../calc-engine/tests/fixtures/excel-calc-probes/P14/eng16-20261105/inputs/eng16-coupled_observed_peer-Range-A1-D1.xlsm"),
        "eng16-coupled_pure_peer-Range-A1-B1" => include_bytes!("../../calc-engine/tests/fixtures/excel-calc-probes/P14/eng16-20261105/inputs/eng16-coupled_pure_peer-Range-A1-B1.xlsm"),
        "eng16-coupled_pure_peer-Range-A1-D1" => include_bytes!("../../calc-engine/tests/fixtures/excel-calc-probes/P14/eng16-20261105/inputs/eng16-coupled_pure_peer-Range-A1-D1.xlsm"),
        _ => panic!("unknown eng16 input {name}"),
    }
}

/// Original formulas/seeds and recorded callback bodies through the shared host
/// scheduler; this isolates engine behavior rather than claiming file-VBA setup.
#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn dc_hsa5_11__eng16_scc_counts_mid_cycle_reads_and_command_profiles() {
    use calc_host::{CalcScope, EvalSelection, WorkBudget};
    use std::{cell::RefCell, rc::Rc};
    let records = std::str::from_utf8(include_bytes!(
        "../../calc-engine/tests/fixtures/excel-calc-probes/P14/eng16-20261105/observations.jsonl"
    )).unwrap().lines().map(|line| serde_json::from_str::<serde_json::Value>(line).unwrap()).collect::<Vec<_>>();
    assert_eq!(records.len(), 16);
    let mut differences = Vec::new();
    let mut checked = 0;
    for record in &records {
        let name = record["condition"].as_str().unwrap();
        let operation = record["specification"]["operation"].as_str().unwrap();
        for budget in [usize::MAX, 1, 3] {
            let mut book = HostSession::open(Arc::new(MemorySource::new(eng16_input(name).to_vec())), &OpenPolicy::default(),
                HostOptions { calc_mode: Some(CalcMode::Manual), ..HostOptions::default() }).unwrap();
            book.set_workbook_events_enabled(false);
            book.set_calculation_budget(WorkBudget::evaluations(budget));
            let sheet = book.host().sheets()[0].id();
            let at = |column| CellKey::new(sheet, 1, column).unwrap();
            let entries = Rc::new(RefCell::new(Vec::new()));
            let mut functions = calc_fml::CallableTable::new();
            for (function, pair) in [("PacketStep", false), ("PacketPairVariant", true)] {
                functions.register(function, Rc::new(P14Callback {
                    pair, sheet, entries: entries.clone(), pause: None, independent: None,
                })).unwrap();
            }
            book.set_user_functions(functions);
            book.take_calculation_events();
            for command in record["commands"].as_array().unwrap() {
                entries.borrow_mut().clear();
                let number = command["command"].as_u64().unwrap();
                let intent = match operation {
                    "Full" => SkinIntent::Calculate { scope: CalcScope::Workbook, selection: EvalSelection::ForceAll },
                    "Rebuild" => SkinIntent::Rebuild,
                    "Range" => SkinIntent::CalculateRange { areas: vec![calc_value::GridArea::new(at(1), at(
                        if record["specification"]["range"] == "A1:B1" { 2 } else { 4 }
                    )).unwrap()] },
                    other => panic!("unexpected recorded command {other}"),
                };
                let receipt = book.dispatch(intent);
                if !receipt.accepted() { differences.push(format!("{name} budget{budget} command{number}: admission {:?}", receipt.error)); }
                p14_advance(&mut book);
                for cell in command["cells"].as_array().unwrap().iter().filter(|c| c["formula"].as_str().is_some_and(|f| !f.is_empty())) {
                    let column = cell["column"].as_u64().unwrap() as u32;
                    let actual = p14_visible(&book, 1, column).0;
                    let expected = p14_value(&cell["value"]);
                    if actual != expected { differences.push(format!("{name} budget{budget} command{number} column{column}: {actual:?} expected {expected:?}")); }
                    checked += 1;
                }
                let wanted = p14_callback_entries(command["events"].as_array().unwrap());
                if *entries.borrow() != wanted {
                    differences.push(format!("{name} budget{budget} command{number}: {} entries expected {}; first {:?} expected {:?}",
                        entries.borrow().len(), wanted.len(), entries.borrow().iter().take(2).collect::<Vec<_>>(), wanted.iter().take(2).collect::<Vec<_>>()));
                }
                let events = book.take_calculation_events();
                let preliminary: Vec<_> = events.iter().filter_map(|event| {
                    if let calc_host::CalcEvent::Reevaluated { previous, next, invocations } = event {
                        Some((previous, next, invocations))
                    } else { None }
                }).collect();
                if name.contains("pair_variant") && operation != "Range" {
                    assert_eq!(preliminary.len(), 1, "{name} command{number}");
                    let (previous, next, calls) = preliminary[0];
                    assert_eq!(previous.evaluation, 0);
                    assert_eq!(next.evaluation, 1);
                    assert_eq!(next.reason, calc_engine::contract::ReevaluationReason::ProvisionalArguments);
                    assert_eq!(calls.len(), 1);
                    assert!(calls[0].completed);
                } else { assert!(preliminary.is_empty(), "{name} command{number}"); }
                if let Some(done) = events.iter().find_map(|event| if let calc_host::CalcEvent::Completed(done) = event { Some(done) } else { None }) {
                    if operation == "Range" {
                        let required = if record["specification"]["range"] == "A1:B1" { 2 } else { 4 };
                        let calculated = usize::from(required == 4 && name.contains("pair_variant"));
                        assert_eq!(done.coverage.required, required);
                        if (done.coverage.calculated, done.coverage.refused) != (calculated, required - calculated) {
                            differences.push(format!("{name} command{number}: coverage {done:?}"));
                        }
                        assert_eq!(done.outcome, calc_engine::contract::CompletionOutcome::Incomplete);
                    } else {
                        assert_eq!(done.coverage.calculated, 4);
                        assert!(done.coverage.uncertified > 0, "iteration still has no SliceV1 invocation certificates");
                    }
                } else { differences.push(format!("{name} command{number}: no completion receipt")); }
            }
        }
    }
    assert_eq!(checked, 384);
    assert!(differences.is_empty(), "{} differences; first:\n{}", differences.len(), differences.iter().take(24).cloned().collect::<Vec<_>>().join("\n"));
}

/// DNA lifecycle controls for the measured scalar prefix, rather than new Excel observations.
#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn dc_hsa5_11__iterative_provisional_prefix_survives_yields_and_cancellation() {
    use std::{cell::{Cell, RefCell}, rc::Rc};
    use calc_host::{CalcScope, EvalSelection, WorkBudget};
    for budget in [1, 3] {
        for cancel in [false, true] {
            let mut book = session_without_recalc(eng16_input("eng16-pair_variant-Full"));
            let sheet = book.host().sheets()[0].id();
            let entries = Rc::new(RefCell::new(Vec::new()));
            let release = Rc::new(Cell::new(false));
            let mut functions = calc_fml::CallableTable::new();
            functions.register("PacketPairVariant", Rc::new(P14Callback {
                pair: true, sheet, entries: entries.clone(), pause: Some(release.clone()), independent: None,
            })).unwrap();
            book.set_user_functions(functions);
            book.set_calculation_budget(WorkBudget::evaluations(budget));
            book.take_calculation_events();
            let before = p14_visible(&book, 1, 4);
            assert!(book.dispatch(SkinIntent::Calculate { scope: CalcScope::Workbook, selection: EvalSelection::ForceAll }).accepted());
            for _ in 0..20 {
                if !entries.borrow().is_empty() { break; }
                assert!(!matches!(book.advance_calculation(), calc_host::HostProgress::Failed(_)));
            }
            assert_eq!(entries.borrow().len(), 1);
            assert_eq!(entries.borrow()[0].reads, vec![CalcValue::empty(); 2]);
            for _ in 0..3 { book.advance_calculation(); }
            assert_eq!(entries.borrow().len(), 1, "physical retries must retain one entry");
            assert_eq!(p14_visible(&book, 1, 4), before, "no preliminary publication");
            assert!(!book.take_calculation_events().iter().any(|event| matches!(event, calc_host::CalcEvent::Completed(_))));
            if cancel {
                book.cancel_calculation();
                assert_eq!(entries.borrow().len(), 1);
                assert_eq!(p14_visible(&book, 1, 4), before);
                let events = book.take_calculation_events();
                assert!(events.iter().any(|event| matches!(event,
                    calc_host::CalcEvent::Completed(done) if done.outcome == calc_engine::contract::CompletionOutcome::Cancelled
                        && done.coverage.calculated == 0)));
            } else {
                release.set(true);
                p14_advance(&mut book);
                assert_eq!(entries.borrow().len(), 6);
                assert_eq!(entries.borrow()[0].reads, vec![CalcValue::empty(); 4]);
                assert_eq!(p14_visible(&book, 1, 1).0.as_number(), Some(14.0));
                assert_eq!(p14_visible(&book, 1, 2).0.as_number(), Some(15.0));
            }
        }
    }
}

#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn dc_hsa5_11__iterative_prefix_semantic_wait_stages_the_first_sweep() {
    use std::{cell::RefCell, rc::Rc};
    use calc_host::{CalcScope, EvalSelection, WorkBudget};
    for budget in [1, 3] {
        let mut book = session_without_recalc(eng16_input("eng16-pair_variant-Full"));
        let sheet = book.host().sheets()[0].id();
        let at = |column| CellKey::new(sheet, 1, column).unwrap();
        set_iteration(&mut book, IterationSettings::new(true, 1, 0.001));
        p14_enter(&mut book, 1, 5, "=40+2");
        let entries = Rc::new(RefCell::new(Vec::new()));
        let mut functions = calc_fml::CallableTable::new();
        functions.register("PacketPairVariant", Rc::new(P14Callback {
            pair: true, sheet, entries: entries.clone(), pause: None, independent: Some(at(5)),
        })).unwrap();
        book.set_user_functions(functions);
        book.set_calculation_budget(WorkBudget::evaluations(budget));
        book.take_calculation_events();
        assert!(book.dispatch(SkinIntent::Calculate { scope: CalcScope::Workbook, selection: EvalSelection::ForceAll }).accepted());
        p14_advance(&mut book);
        assert_eq!(entries.borrow().len(), 2);
        assert_eq!(entries.borrow()[0].reads, vec![CalcValue::empty(); 5]);
        assert_eq!(entries.borrow()[1].reads, [6., 7., 6., 7., 42.].map(CalcValue::number));
        assert_eq!(p14_visible(&book, 1, 4).0.as_number(), Some(0.0));
        let events = book.take_calculation_events();
        assert!(events.iter().any(|event| matches!(event, calc_host::CalcEvent::Completed(done)
            if done.coverage.calculated == 5 && done.coverage.refused == 0)));
    }
}

#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn dc_hsa5_11__cached_iterative_array_reads_refuse_unknown_shapes() {
    use std::{cell::RefCell, rc::Rc};
    for budget in [1, 3] {
        let mut book = session_without_recalc(eng16_input("eng16-pair_variant-Full"));
        let sheet = book.host().sheets()[0].id();
        p14_enter(&mut book, 1, 1, "=IF(TRUE,SEQUENCE(2),B1)");
        let mut functions = calc_fml::CallableTable::new();
        functions.register("PacketPairVariant", Rc::new(P14Callback {
            pair: true, sheet, entries: Rc::new(RefCell::new(Vec::new())), pause: None, independent: None,
        })).unwrap();
        book.set_user_functions(functions);
        book.set_calculation_budget(calc_host::WorkBudget::evaluations(budget));
        assert!(book.dispatch(SkinIntent::Rebuild).accepted());
        p14_advance(&mut book);
        let page = project(book.host(), &ProjectionRequest { cells: vec![CellKey::new(sheet, 1, 4).unwrap()] }).unwrap();
        assert!(matches!(page.recalc_failure(), Some(calc_host::RecalcFailed::Failed(
            calc_engine::RecalcFailure::NotModeledScope { owner: "dc-hsa5.11", .. }
        ))), "{:?}", page.recalc_failure());
        assert!(matches!(page.cells()[0].value(), ProjectedValue::Stale(_)), "{:?}", page.cells()[0]);
    }
}

#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn dc_hsa5_11__a_pre_callback_wait_keeps_dynamic_scc_members_unique() {
    use std::{cell::RefCell, rc::Rc};
    for budget in [1, 3] {
        let mut book = session_without_recalc(eng16_input("eng16-pair_variant-Full"));
        let sheet = book.host().sheets()[0].id();
        set_iteration(&mut book, IterationSettings::new(true, 1, 0.001));
        let entries = Rc::new(RefCell::new(Vec::new()));
        let mut functions = calc_fml::CallableTable::new();
        functions.register("PacketPairVariant", Rc::new(P14Callback {
            pair: true, sheet, entries: entries.clone(), pause: None, independent: None,
        })).unwrap();
        book.set_user_functions(functions);
        p14_enter(&mut book, 1, 4, "=INDIRECT(\"E1\")+PacketPairVariant(A1,B1)");
        p14_enter(&mut book, 1, 5, "=D1+1");
        entries.borrow_mut().clear();
        book.set_calculation_budget(calc_host::WorkBudget::evaluations(budget));
        assert!(book.dispatch(SkinIntent::Rebuild).accepted());
        p14_advance(&mut book);
        assert_eq!(entries.borrow().len(), 1);
        assert_eq!(p14_visible(&book, 1, 4).0.as_number(), Some(0.0));
        assert_eq!(p14_visible(&book, 1, 5).0.as_number(), Some(1.0));
    }
}
