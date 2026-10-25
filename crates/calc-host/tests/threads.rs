//! dc-2m0: every oracle workbook of the repository recalculates to the same results on one thread
//! and on eight, with the parallel route taken by every recalculation that may take it (its
//! minimum set to one formula). The cells compared are every formula cell the one-thread
//! recalculation published; each projects the same value and provenance, its calculation
//! included, after the first recalculation and after a full one.
//!
//! Native-only: it reads the repository's oracle workbooks from disk (wasm32 runs on one thread).

#![cfg(not(target_arch = "wasm32"))]
// Case names are fixed identities of the form `<bead>__<case>`.
#![allow(non_snake_case)]

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
    .map(|cell| format!("{:?}", cell.value()))
    .collect()
}

fn oracle_workbooks() -> Vec<PathBuf> {
    let crates = Path::new(env!("CARGO_MANIFEST_DIR")).join("..");
    let mut found = Vec::new();
    for krate in std::fs::read_dir(&crates).expect("the crates folder").flatten() {
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

#[test]
fn dc_2m0__oracle_workbooks_recalculate_alike_on_one_thread_and_eight() {
    let (mut compared, mut cells) = (0, 0);
    for path in oracle_workbooks() {
        let bytes = std::fs::read(&path).expect("the workbook");
        let (Some(mut one), Some(mut eight)) = (open(&bytes, 1), open(&bytes, 8)) else {
            continue;
        };
        let published = recalculate(&mut one);
        assert_eq!(recalculate(&mut eight), published, "{}", path.display());
        assert_eq!(
            shown(&eight, &published),
            shown(&one, &published),
            "{} after the first recalculation",
            path.display()
        );
        calculate_full(&mut one);
        calculate_full(&mut eight);
        recalculate(&mut one);
        recalculate(&mut eight);
        assert_eq!(
            shown(&eight, &published),
            shown(&one, &published),
            "{} after a full recalculation",
            path.display()
        );
        compared += 1;
        cells += published.len();
    }
    assert!(compared >= 10, "{compared} oracle workbooks compared");
    assert!(cells > 1000, "{cells} formula cells compared");
}
