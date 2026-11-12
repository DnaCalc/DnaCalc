//! dc-2m0: every oracle workbook of the repository recalculates to the same results on one thread
//! and on four, eight and sixteen, with the parallel route taken by every recalculation that may take it (its
//! minimum set to one formula). The cells compared are every formula cell the one-thread
//! recalculation published; each projects the same value and provenance, its calculation
//! included, after the first recalculation and after a full one.
//!
//! Native-only: it reads the repository's oracle workbooks from disk (wasm32 runs on one thread).

#![cfg(not(target_arch = "wasm32"))]
// Case names are fixed identities of the form `<bead>__<case>`.
#![allow(non_snake_case)]

mod contract_support;

use calc_host::value::CellKey;
use calc_host::{
    CalcMode, HostOptions, HostProgress, HostState, MemorySource, OpenPolicy, PackageRead,
    ProjectionRequest, Unobserved, ValueChange, WorkBudget, calculate_full, project, step,
};
use std::path::{Path, PathBuf};
use std::sync::Arc;

fn open(bytes: &[u8], threads: usize) -> Option<HostState> {
    let source: Arc<dyn PackageRead> = Arc::new(MemorySource::new(bytes.to_vec()));
    let mut host = HostState::open(
        source,
        &OpenPolicy::default(),
        HostOptions {
            calc_mode: Some(CalcMode::Automatic),
            ..HostOptions::default()
        },
    )
    .ok()?;
    host.set_threads(threads);
    host.set_parallel_minimum(1, 1);
    Some(host)
}

/// The cells a recalculation of `host` published.
fn recalculate(host: &mut HostState) -> Vec<CellKey> {
    loop {
        match step(host, WorkBudget::UNLIMITED, &mut Unobserved) {
            HostProgress::Calculating { .. } => {}
            HostProgress::Published(delta) => {
                return delta
                    .changes()
                    .iter()
                    .filter_map(|change| match change {
                        ValueChange::Published { at, .. } => Some(*at),
                        ValueChange::Removed { .. } => None,
                    })
                    .collect();
            }
            _ => return Vec::new(),
        }
    }
}

fn shown(host: &HostState, cells: &[CellKey]) -> Vec<String> {
    project(
        host,
        &ProjectionRequest {
            cells: cells.to_vec(),
        },
    )
    .expect("the projection")
    .cells()
    .iter()
    .map(|cell| contract_support::comparable(cell.value()))
    .collect()
}

fn oracle_workbooks() -> Vec<PathBuf> {
    let crates = Path::new(env!("CARGO_MANIFEST_DIR")).join("..");
    let mut found = Vec::new();
    for krate in std::fs::read_dir(&crates)
        .expect("the crates folder")
        .flatten()
    {
        let fixtures = krate.path().join("tests").join("fixtures");
        let Ok(cases) = std::fs::read_dir(&fixtures) else {
            continue;
        };
        for case in cases.flatten() {
            let path = case.path().join("oracle.xlsx");
            if path.is_file() {
                found.push(path);
            }
        }
    }
    found.sort();
    found
}

/// Native-only: exercises native workers across a wide level and late dynamic reads.
#[test]
fn dc_dxm__wide_levels_and_late_reads_keep_serial_values_and_certificates() {
    use calc_host::{CellEdit, HostCommand, dispatch, interpret_entry};
    let bytes = calc_host::blank_package();
    let mut one = open(&bytes, 1).expect("a new workbook");
    let mut several = open(&bytes, 8).expect("a new workbook");
    let sheet = one.sheets()[0].id();
    let at = |row| CellKey::new(sheet, row, 1).unwrap();
    let edit = |row, text: &str| CellEdit::Set {
        at: at(row),
        input: interpret_entry(text).unwrap().unwrap(),
    };
    let edits: Vec<_> = (1..=70_000)
        .map(|row| {
            edit(
                row,
                match row {
                    2 => "=INDIRECT(\"A70000\")+1",
                    69_999 => "=INDIRECT(\"A1\")+2",
                    _ => "=ROW()+1",
                },
            )
        })
        .collect();
    for host in [&mut one, &mut several] {
        let base = host.revision();
        dispatch(
            host,
            HostCommand {
                base,
                edits: edits.clone(),
            },
        )
        .unwrap();
    }
    let cells: Vec<_> = (1..=70_000).map(at).collect();
    for phase in 0..3 {
        if phase == 1 {
            calculate_full(&mut one);
            calculate_full(&mut several);
        } else if phase == 2 {
            for host in [&mut one, &mut several] {
                let base = host.revision();
                dispatch(
                    host,
                    HostCommand {
                        base,
                        edits: vec![edit(70_000, "=42")],
                    },
                )
                .unwrap();
            }
        }
        let published = recalculate(&mut one);
        assert!(!published.is_empty());
        if phase < 2 {
            assert_eq!(published.len(), 70_000);
        }
        assert_eq!(recalculate(&mut several), published);
        assert_eq!(
            shown(&one, &cells),
            shown(&several, &cells),
            "phase {phase}"
        );
        let values = project(
            &several,
            &ProjectionRequest {
                cells: vec![at(1), at(2), at(69_999), at(70_000)],
            },
        )
        .unwrap();
        let expected = if phase == 2 {
            [2., 43., 4., 42.]
        } else {
            [2., 70_002., 4., 70_001.]
        };
        for (cell, number) in values.cells().iter().zip(expected) {
            assert!(
                matches!(cell.value(), calc_host::ProjectedValue::Calculated { value, .. } if value.as_number() == Some(number)),
                "phase {phase}: {:?}",
                cell.value()
            );
        }
    }
}

#[test]
fn dc_2m0__oracle_workbooks_recalculate_alike_on_one_thread_and_several() {
    let (mut compared, mut cells) = (0, 0);
    for path in oracle_workbooks() {
        let bytes = std::fs::read(&path).expect("the workbook");
        let Some(mut one) = open(&bytes, 1) else {
            continue;
        };
        let published = recalculate(&mut one);
        let first = shown(&one, &published);
        calculate_full(&mut one);
        recalculate(&mut one);
        let full = shown(&one, &published);
        for threads in [4, 8, 16] {
            let mut several = open(&bytes, threads).expect("the workbook opens again");
            let at = format!("{} on {threads} threads", path.display());
            assert_eq!(recalculate(&mut several), published, "{at}");
            assert_eq!(
                shown(&several, &published),
                first,
                "{at}, the first recalculation"
            );
            calculate_full(&mut several);
            recalculate(&mut several);
            assert_eq!(
                shown(&several, &published),
                full,
                "{at}, a full recalculation"
            );
        }
        compared += 1;
        cells += published.len();
    }
    assert!(compared >= 10, "{compared} oracle workbooks compared");
    assert!(cells > 1000, "{cells} formula cells compared");
}

/// dc-dxm: competing spills on several threads. Five dynamic arrays (two in column A, one across
/// row 1 from C, and a column from E3 crossing a row from C6, which wait for each other) are
/// resized, moved and cleared at random, so that they block, release and wait for each other,
/// with 160 ordinary formulas reading their areas (whole ranges, INDEX into them and single
/// cells). Host A recalculates on four threads with every
/// recalculation taking the parallel route when it may; host B takes the same edits on one
/// thread. Which of two competing spills fills its area depends on what the last recalculation
/// published, so B has the same history rather than being a fresh open. After every edit, and
/// after a full recalculation every tenth, every cell of A1:J40 must project alike in both.
#[test]
fn dc_dxm__competing_spills_recalculate_alike_on_one_thread_and_four() {
    use calc_host::value::GridArea;
    use calc_host::{
        ArrayFormula, ArrayKind, CellEdit, CellInput, HostCommand, ProjectedValue, dispatch,
        interpret_entry,
    };
    let bytes = calc_host::blank_package();
    let (mut a, mut b) = (
        open(&bytes, 4).expect("a new workbook"),
        open(&bytes, 1).expect("a new workbook"),
    );
    let sheet = a.sheets()[0].id();
    let at = |row: u32, column: u32| CellKey::new(sheet, row, column).expect("a cell");
    let apply = |host: &mut HostState, edit: &CellEdit| {
        let base = host.revision();
        dispatch(
            host,
            HostCommand {
                base,
                edits: vec![edit.clone()],
            },
        )
        .unwrap_or_else(|e| panic!("the edit {edit:?} commits: {e}"));
    };
    let formula = |text: &str| match interpret_entry(text) {
        Ok(Some(CellInput::Formula(formula))) => formula,
        other => panic!("{text:?} is not a formula: {other:?}"),
    };
    let set = |cell: CellKey, text: &str| match interpret_entry(text).expect("an entry") {
        Some(input) => CellEdit::Set { at: cell, input },
        None => CellEdit::Clear {
            area: GridArea::cell(cell),
        },
    };
    let array = |cell: CellKey, text: &str| CellEdit::SetArray {
        array: ArrayFormula::new(
            GridArea::cell(cell),
            ArrayKind::Dynamic { collapsed: false },
        ),
        formula: formula(text),
    };
    let shown = |host: &HostState, cells: &[CellKey]| -> Vec<String> {
        project(
            host,
            &ProjectionRequest {
                cells: cells.to_vec(),
            },
        )
        .expect("the projection")
        .cells()
        .iter()
        .map(|cell| match cell.value() {
            ProjectedValue::Calculated { value, .. } => format!("calculated {value:?}"),
            ProjectedValue::NotModeled { refusal, .. } => format!("refused {refusal:?}"),
            other => format!("{other:?}"),
        })
        .collect()
    };
    // Sizes in Z1:Z5; the arrays' anchors, each with the formula it holds when entered.
    const Z: u32 = 26;
    let anchors = [
        (at(1, 1), "=_xlfn.SEQUENCE(Z1)"),
        (at(4, 1), "=_xlfn.SEQUENCE(Z2)*10"),
        (at(1, 3), "=_xlfn.SEQUENCE(1,Z3)*100"),
        (at(3, 5), "=_xlfn.SEQUENCE(Z4)*1000"),
        (at(6, 3), "=_xlfn.SEQUENCE(1,MOD(Z4+Z5,6)+1)*10000"),
    ];
    let mut setup = Vec::new();
    for row in 1..=5 {
        setup.push(set(at(row, Z), &row.to_string()));
    }
    for (anchor, text) in anchors {
        setup.push(array(anchor, text));
    }
    for row in 1..=40u32 {
        setup.push(set(
            at(row, 7),
            &format!(
                "=SUM($A$1:$A$12)*{row}+INDEX($C$1:$F$1,1,{})+A{}",
                row % 4 + 1,
                row % 12 + 1
            ),
        ));
        setup.push(set(
            at(row, 8),
            &format!("=G{row}+COUNT($D$1:$D$12)+B{}", row % 5 + 1),
        ));
        setup.push(set(at(row, 9), &format!("=H{row}*2+D{}", row % 9 + 1)));
        setup.push(set(at(row, 10), &format!("=I{row}+E{}+F6", row % 9 + 1)));
    }
    for edit in &setup {
        apply(&mut a, edit);
        apply(&mut b, edit);
    }
    let cells: Vec<CellKey> = (1..=40)
        .flat_map(|row| (1..=10).map(move |column| (row, column)))
        .map(|(row, column)| at(row, column))
        .collect();
    let mut draws = 0x2545_f491_4f6c_dd1du64;
    let mut below = |n: u64| {
        draws ^= draws << 13;
        draws ^= draws >> 7;
        draws ^= draws << 17;
        draws % n
    };
    recalculate(&mut a);
    recalculate(&mut b);
    assert_eq!(shown(&a, &cells), shown(&b, &cells), "after the setup");
    for round in 1..=150u32 {
        let pick = below(5) as usize;
        let edit = match below(4) {
            0 | 1 => set(at(pick as u32 + 1, Z), &(below(8) + 1).to_string()),
            2 => {
                // Moved down its column (or along row 1) by up to three cells.
                let (anchor, text) = anchors[pick];
                let shift = below(4) as u32;
                let moved = if pick == 2 || pick == 4 {
                    at(anchor.row(), anchor.column() + shift)
                } else {
                    at(anchor.row() + shift, anchor.column())
                };
                array(moved, text)
            }
            _ => CellEdit::Clear {
                area: GridArea::new(at(1, 1), at(12, 6)).expect("an area"),
            },
        };
        let edits = if matches!(edit, CellEdit::Clear { .. }) {
            // Cleared, then the four arrays entered again in a drawn order.
            let mut again = vec![edit];
            let first = below(5) as usize;
            for offset in 0..5 {
                let (anchor, text) = anchors[(first + offset) % 5];
                again.push(array(anchor, text));
            }
            again
        } else {
            vec![edit]
        };
        for edit in &edits {
            apply(&mut a, edit);
            apply(&mut b, edit);
            recalculate(&mut a);
            recalculate(&mut b);
            assert_eq!(
                shown(&a, &cells),
                shown(&b, &cells),
                "round {round}, {edit:?}"
            );
        }
        if round % 10 == 0 {
            calculate_full(&mut a);
            calculate_full(&mut b);
            recalculate(&mut a);
            recalculate(&mut b);
            assert_eq!(shown(&a, &cells), shown(&b, &cells), "round {round}, full");
        }
    }
}

/// dc-1nbp.24: level workers hold no callable table, so a formula with a user call is left to the
/// owner. The cells and values at four threads equal the one-thread run, with no refusal.
///
/// Native-only: runs the engine's native worker threads.
#[test]
fn dc_1nbp_24__user_calls_on_a_parallel_level_match_one_thread() {
    use calc_host::value::CalcValue;
    use calc_host::{CellEdit, HostCommand, dispatch, interpret_entry};
    #[derive(Debug)]
    struct Triple;
    impl calc_fml::UserFunction for Triple {
        fn max_parallel_calls(&self) -> usize {
            8
        }
        fn call(&self, arguments: &[CalcValue]) -> Result<CalcValue, String> {
            Ok(CalcValue::number(arguments[0].as_number().unwrap_or(0.0) * 3.0))
        }
    }
    let bytes = calc_host::blank_package();
    let mut one = open(&bytes, 1).expect("a new workbook");
    let mut four = open(&bytes, 4).expect("a new workbook");
    let sheet = one.sheets()[0].id();
    let at = |row| CellKey::new(sheet, row, 1).unwrap();
    // One wide level: plain formulas and user calls side by side.
    let edits: Vec<_> = (1..=200)
        .map(|row| CellEdit::Set {
            at: at(row),
            input: interpret_entry(&if row % 2 == 0 {
                format!("=Triple({row})")
            } else {
                format!("=ROW()+1")
            })
            .unwrap()
            .unwrap(),
        })
        .collect();
    for host in [&mut one, &mut four] {
        let mut functions = calc_fml::CallableTable::new();
        functions
            .register("Triple", std::rc::Rc::new(Triple))
            .unwrap();
        host.set_user_functions(functions);
        let base = host.revision();
        dispatch(
            host,
            HostCommand {
                base,
                edits: edits.clone(),
            },
        )
        .unwrap();
    }
    let cells: Vec<_> = (1..=200).map(at).collect();
    let published = recalculate(&mut one);
    assert_eq!(published.len(), 200);
    assert_eq!(recalculate(&mut four), published);
    // Provenance ids (invocation numbers, callable table identity) differ between two hosts by
    // construction; the values and their kind are what the two runs must share.
    let numbers = |host: &HostState| -> Vec<Option<f64>> {
        project(host, &ProjectionRequest { cells: cells.clone() })
            .unwrap()
            .cells()
            .iter()
            .map(|cell| match cell.value() {
                calc_host::ProjectedValue::Calculated { value, .. } => value.as_number(),
                other => panic!("not calculated: {other:?}"),
            })
            .collect()
    };
    let serial = numbers(&one);
    assert_eq!(serial, numbers(&four));
    assert_eq!(serial[1], Some(6.0));
    let values = project(&four, &ProjectionRequest { cells: vec![at(2), at(3)] }).unwrap();
    for (cell, number) in values.cells().iter().zip([6., 4.]) {
        assert!(
            matches!(cell.value(), calc_host::ProjectedValue::Calculated { value, .. } if value.as_number() == Some(number)),
            "{:?}",
            cell.value()
        );
    }
}
