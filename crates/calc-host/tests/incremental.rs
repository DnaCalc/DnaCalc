//! dc-dxm: incremental recalculation against a fresh open, the differential the review of dc-10d9
//! asked for. Host A recalculates after every edit, so the engine's kept state (the readers cache
//! of each formula, published results, spills, dynamic prerequisites) carries from one
//! recalculation to the next; host B is a fresh open of the same package given the same edits
//! and recalculated once. Every compared cell must project the same value or refusal in both.
//! The edits cover INDIRECT, OFFSET and INDEX references, a SEQUENCE spill blocked, released and
//! reshaped, formula edits that make and break a circular reference, and value edits, on calc-doc's
//! D1 fixture (embedded, so the test runs natively and in the browsers). Ported from the review's
//! harness (`difftest`, synthetic mode).
//!
//! While a circular reference stands, the two hosts are not compared: with iteration off a
//! formula on it, or reading it, keeps the value it last had (dc-10y), which depends on the
//! history a fresh open does not have. The test tracks the edits that make one (B pointing at
//! its own row's M, or INDIRECTs of column B that lead back to themselves) and compares once
//! every one is broken again.

// Case names are fixed identities of the form `<bead>__<case>`.
#![allow(non_snake_case)]

use calc_host::value::{CellKey, GridArea, SheetId};
use calc_host::{
    ArrayFormula, ArrayKind, CalcMode, CellEdit, CellInput, HostCommand, HostOptions,
    HostProgress, HostState, MemorySource, OpenPolicy, PackageRead, ProjectedValue,
    ProjectionRequest, Unobserved, WorkBudget, dispatch, interpret_entry, project, step,
};
use std::sync::Arc;

fn fixture() -> Vec<u8> {
    include_bytes!("../../calc-doc/tests/fixtures/cdoc_m1_2/d1.saved.xlsx").to_vec()
}

fn open(bytes: &[u8]) -> HostState {
    let source: Arc<dyn PackageRead> = Arc::new(MemorySource::new(bytes.to_vec()));
    HostState::open(
        source,
        &OpenPolicy::default(),
        HostOptions {
            calc_mode: Some(CalcMode::Automatic),
            ..HostOptions::default()
        },
    )
    .expect("it opens")
}

/// Recalculates to the end; how many formulas it published.
fn recalculate(host: &mut HostState) -> usize {
    loop {
        match step(host, WorkBudget::UNLIMITED, &mut Unobserved) {
            HostProgress::Calculating { .. } => {}
            HostProgress::Published(delta) => return delta.changes().len(),
            HostProgress::Current => return 0,
            other => panic!("the recalculation did not publish: {other:?}"),
        }
    }
}

fn apply(host: &mut HostState, edit: &CellEdit) {
    let base = host.revision();
    dispatch(
        host,
        HostCommand {
            base,
            edits: vec![edit.clone()],
        },
    )
    .unwrap_or_else(|e| panic!("the edit {edit:?} commits: {e}"));
}

/// What a cell projects, as text both hosts can be compared by.
fn shown(value: &ProjectedValue) -> String {
    match value {
        ProjectedValue::Calculated { value, .. } => format!("calculated {value:?}"),
        ProjectedValue::NotModeled { refusal, .. } => format!("refused {refusal:?}"),
        ProjectedValue::Literal(value) => format!("literal {value:?}"),
        ProjectedValue::Empty => "empty".to_owned(),
        ProjectedValue::FileCached(_) => "file cache".to_owned(),
        ProjectedValue::Stale(_) => "stale".to_owned(),
    }
}

fn snapshot(host: &HostState, cells: &[CellKey]) -> Vec<String> {
    project(
        host,
        &ProjectionRequest {
            cells: cells.to_vec(),
        },
    )
    .expect("the projection")
    .cells()
    .iter()
    .map(|cell| shown(cell.value()))
    .collect()
}

/// The cells of host `a` that project otherwise than in a fresh open given `edits`, with both.
fn mismatches(
    bytes: &[u8],
    a: &HostState,
    edits: &[CellEdit],
    cells: &[CellKey],
) -> Vec<(CellKey, String, String)> {
    let mut b = open(bytes);
    for edit in edits {
        apply(&mut b, edit);
    }
    recalculate(&mut b);
    snapshot(a, cells)
        .into_iter()
        .zip(snapshot(&b, cells))
        .zip(cells)
        .filter(|((x, y), _)| x != y)
        .map(|((x, y), at)| (*at, x, y))
        .collect()
}

/// A cell entry as the user types it; an empty entry clears the cell.
fn set(at: CellKey, text: &str) -> CellEdit {
    match interpret_entry(text).unwrap_or_else(|e| panic!("the entry {text:?}: {e:?}")) {
        Some(input) => CellEdit::Set { at, input },
        None => CellEdit::Clear {
            area: GridArea::cell(at),
        },
    }
}

/// A dynamic-array formula entered at `at`.
fn dynamic_array(at: CellKey, text: &str) -> CellEdit {
    match interpret_entry(text) {
        Ok(Some(CellInput::Formula(formula))) => CellEdit::SetArray {
            array: ArrayFormula::new(GridArea::cell(at), ArrayKind::Dynamic { collapsed: false }),
            formula,
        },
        other => panic!("{text:?} is not a formula: {other:?}"),
    }
}

/// xorshift64, so the edits are the same on every run and platform.
struct Draws(u64);

/// What a row's B cell holds, as far as circular references go.
#[derive(Clone, Copy, PartialEq, Eq)]
enum BForm {
    /// `=M{r}`: M reads E and J, which read B, so B is on a circular reference.
    OwnM,
    /// `=INDIRECT("B"&D{r})`: B reads the B of the row D names.
    Indirect,
    /// Anything else, which reads no B and no M.
    Other,
}

/// Whether the rows' B cells, with D's row numbers, make a circular reference.
fn has_cycle(forms: &[BForm], d: &[u32], r0: u32) -> bool {
    if forms.contains(&BForm::OwnM) {
        return true;
    }
    (0..forms.len()).any(|start| {
        let mut row = start;
        for _ in 0..=forms.len() {
            if forms[row] != BForm::Indirect {
                return false;
            }
            row = (d[row] - r0) as usize;
            if row == start {
                return true;
            }
        }
        true
    })
}

impl Draws {
    fn below(&mut self, n: u32) -> u32 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        (self.0 % u64::from(n)) as u32
    }
}

fn sheet(host: &HostState, name: &str) -> SheetId {
    host.sheets()
        .iter()
        .find(|sheet| sheet.name().text().to_string_lossy() == name)
        .expect("the sheet")
        .id()
}

#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn dc_dxm__recalculating_after_every_edit_matches_a_fresh_open() {
    differential(&fixture(), "Calc", true, 1);
}

/// dc-2m0: the same differential with host A recalculating on four threads, every recalculation
/// of at least one formula taking the parallel route when it may, against the fresh open on one
/// thread. The package is a new workbook (no array formula, which keeps a recalculation on one
/// thread), and column H holds numbers in place of the spill. On wasm32 both run on one thread.
/// dc-dxm: the D1 differential, with its SEQUENCE spill blocked, released and reshaped, on four
/// threads, every recalculation of at least one formula taking the parallel route when it may
/// (array formulas evaluated on the calling thread at their places in the order), against the
/// fresh open on one thread. On wasm32 both run on one thread.
#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn dc_dxm__recalculating_with_spills_on_four_threads_matches_a_fresh_open() {
    differential(&fixture(), "Calc", true, 4);
}

#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn dc_2m0__recalculating_on_four_threads_matches_a_fresh_open() {
    differential(&calc_host::blank_package(), "Sheet1", false, 4);
}

/// The differential on `bytes`, the cells on sheet `name`: with the SEQUENCE spill in column H
/// when `spill`, else numbers there; host A on `threads` threads.
fn differential(bytes: &[u8], name: &str, spill: bool, threads: usize) {
    const R0: u32 = 3001;
    const N: u32 = 20;
    const STEPS: u32 = 200;
    const EVERY: u32 = 5;
    let bytes = bytes.to_vec();
    let mut a = open(&bytes);
    a.set_threads(threads);
    a.set_parallel_minimum(1, 1);
    let calc = sheet(&a, name);
    let at = |row: u32, column: char| {
        CellKey::new(calc, row, u32::from(column) - u32::from('A') + 1).expect("a cell")
    };
    let mut setup = Vec::new();
    for r in R0..R0 + N {
        let last = R0 + N - 1;
        setup.push(set(at(r, 'A'), &format!("{}", r - R0 + 1)));
        setup.push(set(at(r, 'B'), &format!("=A{r}*2+1")));
        setup.push(set(at(r, 'D'), &format!("{}", R0 + (r - R0 + 3) % N)));
        setup.push(set(at(r, 'C'), &format!("=INDIRECT(\"A\"&D{r})")));
        setup.push(set(
            at(r, 'E'),
            &format!("=SUM(OFFSET(A${R0},$F${R0},0,$G${R0},1))+B{r}"),
        ));
        setup.push(set(at(r, 'J'), &format!("=N(H{r})*B{r}")));
        setup.push(set(
            at(r, 'K'),
            &format!("=SUM(A${R0}:INDEX(A${R0}:A${last},$L${R0}))"),
        ));
        setup.push(set(at(r, 'M'), &format!("=C{r}+E{r}+K{r}+J{r}")));
        if spill {
            setup.push(set(at(r, 'O'), &format!("=SUM(_xlfn.ANCHORARRAY(H{R0}))+M{r}")));
        } else {
            setup.push(set(at(r, 'H'), &format!("{}", (r - R0) * 10)));
            setup.push(set(at(r, 'O'), &format!("=SUM(H{R0}:H{last})+M{r}")));
        }
    }
    setup.push(set(at(R0, 'F'), "2"));
    setup.push(set(at(R0, 'G'), "3"));
    setup.push(set(at(R0, 'I'), "4"));
    setup.push(set(at(R0, 'L'), "5"));
    if spill {
        setup.push(dynamic_array(
            at(R0, 'H'),
            &format!("=_xlfn.SEQUENCE($I${R0})*10"),
        ));
    }
    let cells: Vec<CellKey> = (R0..R0 + N + 2)
        .flat_map(|row| ('A'..='P').map(move |column| (row, column)))
        .map(|(row, column)| at(row, column))
        .collect();

    recalculate(&mut a);
    let mut edits = Vec::new();
    for edit in setup {
        apply(&mut a, &edit);
        edits.push(edit);
    }
    recalculate(&mut a);
    let found = mismatches(&bytes, &a, &edits, &cells);
    assert!(found.is_empty(), "after the setup: {found:?}");

    let mut draws = Draws(0x9e37_79b9_7f4a_7c15);
    let mut forms = vec![BForm::Other; N as usize];
    let mut d: Vec<u32> = (0..N).map(|i| R0 + (i + 3) % N).collect();
    let (mut compared, mut with_cycle) = (0, 0);
    for i in 1..=STEPS {
        let r = R0 + draws.below(N);
        let row = (r - R0) as usize;
        let edit = match draws.below(12) {
            0 | 1 => set(at(r, 'A'), &format!("{}", draws.below(100))),
            2 | 11 => {
                d[row] = R0 + draws.below(N);
                set(at(r, 'D'), &format!("{}", d[row]))
            }
            3 => {
                let column = if draws.below(2) == 0 { 'F' } else { 'G' };
                set(at(R0, column), &format!("{}", 1 + draws.below(6)))
            }
            4 => set(at(R0, 'I'), &format!("{}", 1 + draws.below(12))),
            5 => set(at(R0, 'L'), &format!("{}", 1 + draws.below(N))),
            6 => {
                // `=M{r}` makes a circular reference through M, which the other choices break.
                let choices = [
                    format!("=A{r}*2+1"),
                    format!("=A{}-1", r + 1),
                    format!("=M{r}"),
                    format!("=SUM(A{R0}:A{})", R0 + 4),
                    "=5".to_owned(),
                    String::new(),
                    format!("=INDIRECT(\"B\"&D{r})"),
                    format!("=OFFSET(H{R0},{},0)", draws.below(6)),
                ];
                let choice = draws.below(choices.len() as u32) as usize;
                forms[row] = match choice {
                    2 => BForm::OwnM,
                    6 => BForm::Indirect,
                    _ => BForm::Other,
                };
                set(at(r, 'B'), &choices[choice])
            }
            7 => {
                // A value blocks the spill; clearing it releases the spill. Without the spill,
                // the value is one of column H's numbers.
                let blocker = at(R0 + 1 + draws.below(8), 'H');
                if draws.below(2) == 0 || !spill {
                    set(blocker, "99")
                } else {
                    set(blocker, "")
                }
            }
            8 => {
                if draws.below(2) == 0 {
                    set(at(r, 'C'), "")
                } else {
                    set(at(r, 'C'), &format!("=INDIRECT(\"A\"&D{})", R0 + 4))
                }
            }
            9 if spill => dynamic_array(
                at(R0, 'H'),
                &format!("=_xlfn.SEQUENCE($I${R0},{})*10", 1 + draws.below(2)),
            ),
            9 => set(at(R0, 'H'), &format!("{}", draws.below(50))),
            _ => set(
                at(r, 'M'),
                &format!("=C{r}+E{r}+K{r}+J{r}+B{}", R0 + draws.below(N)),
            ),
        };
        apply(&mut a, &edit);
        recalculate(&mut a);
        edits.push(edit);
        if has_cycle(&forms, &d, R0) {
            with_cycle += 1;
        } else if i % EVERY == 0 || compared * EVERY < i / 2 {
            let found = mismatches(&bytes, &a, &edits, &cells);
            assert!(found.is_empty(), "after edit {i}: {found:?}");
            compared += 1;
        }
    }
    // Circular references were made and broken, and the hosts were compared throughout.
    assert!(with_cycle > 0 && with_cycle < STEPS, "{with_cycle} steps with a cycle");
    assert!(compared >= STEPS / EVERY / 3, "{compared} comparisons");
}
