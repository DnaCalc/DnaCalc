//! Excel's recalculation sequence (`crates/calc-vba/tests/fixtures/excel-recalculation/`: 17 stages,
//! 23 formula cells calling probe UDFs that count their own calls) replayed through DNA Calc's
//! route: a `HostSession` in manual calculation mode with the probe module installed as the
//! workbook's VBA project, the stage edits entered as skin intents and each calculation requested
//! as the stage's Excel call is (`Application.Calculate`, `Range.Calculate`, `Worksheet.Calculate`,
//! `CalculateFull`, `CalculateFullRebuild`). Each stage's 23 cell values are compared with the
//! `observations.json` Excel recorded.
//!
//! Cells the engine still answers differently are listed in
//! `excel-recalculation.known-differences.txt` (`<stage>:B<row>`); the test fails on any new
//! difference and on any listed difference that is now fixed.
use calc_host::{
    CalcScope, EvalSelection, HostOptions, HostSession, MemorySource, OpenPolicy, SkinIntent,
};
use std::collections::BTreeSet;
use std::sync::Arc;

const PROBE: &str = include_str!("../../calc-vba/tests/fixtures/excel-recalculation/probe.bas");
const OBSERVATIONS: &str =
    include_str!("../../calc-vba/tests/fixtures/excel-recalculation/observations.json");
const KNOWN: &str = include_str!("excel-recalculation.known-differences.txt");

/// `(stage, the 23 cell texts)` in the order Excel recorded them.
fn observations() -> Vec<(String, Vec<String>)> {
    let mut out: Vec<(String, Vec<String>)> = Vec::new();
    let mut in_cells = false;
    for line in OBSERVATIONS.lines() {
        let trimmed = line.trim();
        if in_cells {
            if trimmed.starts_with(']') {
                in_cells = false;
            } else {
                out.last_mut()
                    .unwrap()
                    .1
                    .push(trimmed.trim_end_matches(',').trim_matches('"').to_string());
            }
        } else if trimmed.starts_with("\"cells\": [") {
            in_cells = true;
        } else if line.starts_with("    \"") && trimmed.ends_with("\": {") {
            out.push((
                trimmed
                    .trim_end_matches("\": {")
                    .trim_matches('"')
                    .to_string(),
                Vec::new(),
            ));
        }
    }
    out
}

fn session() -> HostSession {
    HostSession::open(
        Arc::new(MemorySource::new(calc_doc::blank_package_named("Probe")).named("probe.xlsm")),
        &OpenPolicy::default(),
        HostOptions::default(),
    )
    .unwrap()
}

fn at(session: &HostSession, sheet: usize, row: u32, column: u32) -> calc_host::value::CellKey {
    calc_host::value::CellKey::new(session.host().sheets()[sheet].id(), row, column).unwrap()
}

fn enter(session: &mut HostSession, cell: &str, text: &str) {
    let column = u32::from(cell.as_bytes()[0] - b'A') + 1;
    let row: u32 = cell[1..].parse().unwrap();
    let at = at(session, 0, row, column);
    assert!(
        session
            .dispatch(SkinIntent::EnterCell {
                at,
                text: text.into()
            })
            .accepted(),
        "{cell} = {text}"
    );
}

fn drain(session: &mut HostSession) {
    for _ in 0..10_000 {
        if !session.has_pending_calculation() {
            return;
        }
        let progress = session.advance_calculation();
        assert!(
            !matches!(
                progress,
                calc_host::HostProgress::Failed(_) | calc_host::HostProgress::Waiting { .. }
            ),
            "{progress:?}"
        );
    }
    panic!("calculation did not complete");
}

fn request(session: &mut HostSession, intent: SkinIntent) {
    assert!(session.dispatch(intent).accepted());
    drain(session);
}

fn cell_text(session: &HostSession, row: u32) -> String {
    let key = at(session, 0, row, 2);
    let page = calc_host::project(
        session.host(),
        &calc_host::ProjectionRequest { cells: vec![key] },
    )
    .unwrap();
    match page.cells()[0].value() {
        calc_host::ProjectedValue::Calculated { value, .. } => text_of(value),
        other => format!("{other:?}"),
    }
}

fn text_of(value: &calc_value::CalcValue) -> String {
    if let Some(n) = value.as_number() {
        if n.fract() == 0.0 {
            return format!("{}", n as i64);
        }
        return format!("{n}");
    }
    if let Some(text) = value.as_text() {
        return text.to_string_lossy();
    }
    format!("{value:?}")
}

fn snapshot(session: &HostSession) -> Vec<String> {
    (1..=23).map(|row| cell_text(session, row)).collect()
}

fn range(session: &HostSession, cell: &str) -> calc_value::GridArea {
    let column = u32::from(cell.as_bytes()[0] - b'A') + 1;
    let row: u32 = cell[1..].parse().unwrap();
    let key = at(session, 0, row, column);
    calc_value::GridArea::new(key, key).unwrap()
}

fn probe_source() -> String {
    PROBE
        .split("Private Function Snapshot")
        .next()
        .unwrap()
        .to_string()
}

fn modules(source: &str) -> [calc_host_vba::ModuleSource; 2] {
    [
        calc_host_vba::ModuleSource {
            name: "Probe".into(),
            source: source.into(),
            target: calc_host_vba::ModuleTarget::Standard,
        },
        calc_host_vba::ModuleSource {
            name: "ThisWorkbook".into(),
            source: String::new(),
            target: calc_host_vba::ModuleTarget::Workbook,
        },
    ]
}

/// The unmodified fixture now loads in the document route. The module route still
/// has no ThisWorkbook; the sequence comparison below measures its calculation gaps.
#[test]
fn the_unmodified_probe_loads_as_a_document_and_needs_thisworkbook_as_a_module() {
    let mut s = session();
    let project = calc_host_vba::prepare(
        s.host(),
        "probe.xlsm",
        &modules(&probe_source()),
        100_000,
        Some(calc_vba::Backend::Interpreter),
    )
    .expect("the document route admits the probe's optional host members");
    project.install(&mut s);
    enter(&mut s, "B1", "=PlainProbe(1,42)");
    drain(&mut s);
    assert_eq!(cell_text(&s, 1), "1");
    // The module route has no ThisWorkbook.
    let refusal = calc_vba::load_module_with_backend(
        "Probe",
        &probe_source(),
        100_000,
        calc_vba::Backend::Interpreter,
    )
    .err()
    .expect("the module route has no ThisWorkbook");
    assert!(
        refusal.contains("Variable not defined: ThisWorkbook"),
        "{refusal}"
    );
}

fn replay(backend: calc_vba::Backend) -> Vec<(String, Vec<String>)> {
    let mut s = session();
    s.dispatch(SkinIntent::SetCalcMode(calc_host::CalcMode::Manual));
    // A second sheet, as the probe adds, for the `other-sheet` stage.
    let key = calc_host::SheetKey::new(2);
    let name = calc_host::SheetName::new(calc_host::value::ExcelText::from_utf16_code_units(
        "Sheet2".encode_utf16().collect(),
    ))
    .unwrap();
    assert!(
        s.dispatch(SkinIntent::Edit {
            edits: vec![calc_host::CellEdit::Sheet(calc_host::SheetEdit::Insert {
                key,
                name,
                position: std::num::NonZeroU32::new(2).unwrap(),
            })],
        })
        .accepted()
    );
    // The module route loads the probe with `Application.Volatile`; it has no `ThisWorkbook`, so
    // HiddenProbe's `ThisWorkbook.Worksheets("Probe").Range("Z1")` reads the same cell as
    // `ActiveSheet.Range("Z1")` (the probe sheet is the active one). The document-project route
    // refuses the probe as Excel ran it (`the_unmodified_probe_is_refused`).
    let replayable = probe_source().replace(
        "ThisWorkbook.Worksheets(\"Probe\").Range(\"Z1\").Value2",
        "ActiveSheet.Range(\"Z1\").Value2",
    );
    let functions =
        calc_vba::load_module_with_backend("Probe", &replayable, 100_000, backend).unwrap();
    s.set_user_functions(functions);

    // The setup `RunProbe` performs, in manual calculation mode.
    enter(&mut s, "A1", "10");
    enter(&mut s, "A3", "=A1*0");
    enter(&mut s, "A4", "=RAND()*0");
    enter(&mut s, "A6", "=RAND()");
    for (cell, value) in [
        ("D1", "TRUE"),
        ("D2", "FALSE"),
        ("D3", "TRUE"),
        ("D4", "FALSE"),
    ] {
        enter(&mut s, cell, value);
    }
    enter(&mut s, "Z1", "30");
    let formulas = [
        "=PlainProbe(1,A1)",
        "=VolProbe(2,A1)",
        "=PlainProbe(3,A3)",
        "=PlainProbe(4,1)",
        "=PlainProbe(5,A4)",
        "=ToggleProbe(6,D1,A1)",
        "=ToggleProbe(7,D2,A1)",
        "=ConditionalProbe(8,D3,A1)",
        "=LastWinsProbe(9,A1)",
        "=NestedProbe(10,D4,A1)",
        "=PlainProbe(11,B2*0)",
        "=HiddenProbe()",
        "=NoArgsProbe()",
        "=PlainProbe(14,RAND()*0)",
        "=PlainProbe(15,A6)",
        "=StableVolProbe(A1)",
        "=PlainProbe(17,B16)",
        "=ToggleProbe(18,D1,A1)+ToggleProbe(19,D2,A1)",
        "=ToggleProbe(20,D2,A1)+ToggleProbe(21,D1,A1)",
        "=VolProbe(22,A1)+PlainProbe(23,A1)",
        "=PlainProbe(28,1)",
        "=IF(TRUE,PlainProbe(24,A1),VolProbe(25,A1))",
        "=IF(TRUE,VolProbe(26,A1),PlainProbe(27,A1))",
    ];
    for (i, formula) in formulas.iter().enumerate() {
        enter(&mut s, &format!("B{}", i + 1), formula);
    }

    let mut out = Vec::new();
    let mut stage = |name: &str, s: &HostSession| out.push((name.to_string(), snapshot(s)));
    request(&mut s, SkinIntent::Recalculate);
    stage("baseline", &s);
    request(&mut s, SkinIntent::Recalculate);
    stage("clean", &s);
    enter(&mut s, "A2", "99");
    request(&mut s, SkinIntent::Recalculate);
    stage("unrelated", &s);
    enter(&mut s, "A1", "10");
    request(&mut s, SkinIntent::Recalculate);
    stage("same-input", &s);
    enter(&mut s, "A1", "11");
    request(&mut s, SkinIntent::Recalculate);
    stage("changed-input", &s);
    let b1 = range(&s, "B1");
    request(&mut s, SkinIntent::CalculateRange { areas: vec![b1] });
    stage("range-force", &s);
    let sheet_id = s.host().sheets()[0].id();
    request(
        &mut s,
        SkinIntent::Calculate {
            scope: CalcScope::Sheet(sheet_id),
            selection: EvalSelection::DirtyAndVolatile,
        },
    );
    stage("sheet-clean", &s);
    request(
        &mut s,
        SkinIntent::Calculate {
            scope: CalcScope::Workbook,
            selection: EvalSelection::ForceAll,
        },
    );
    stage("full", &s);
    request(&mut s, SkinIntent::Rebuild);
    stage("rebuild", &s);
    for (cell, value) in [
        ("D1", "FALSE"),
        ("D2", "TRUE"),
        ("D3", "FALSE"),
        ("D4", "TRUE"),
    ] {
        enter(&mut s, cell, value);
    }
    request(&mut s, SkinIntent::Recalculate);
    stage("toggle", &s);
    request(&mut s, SkinIntent::Recalculate);
    stage("toggle-clean", &s);
    // `Application.Volatile True` outside a worksheet function call changes nothing.
    request(&mut s, SkinIntent::Recalculate);
    stage("outside-volatile", &s);
    enter(&mut s, "Z1", "31");
    request(&mut s, SkinIntent::Recalculate);
    stage("hidden-input", &s);
    let other = at(&s, 1, 1, 1);
    assert!(
        s.dispatch(SkinIntent::EnterCell {
            at: other,
            text: "77".into()
        })
        .accepted()
    );
    request(&mut s, SkinIntent::Recalculate);
    stage("other-sheet", &s);
    let b12 = range(&s, "B12");
    request(&mut s, SkinIntent::CalculateRange { areas: vec![b12] });
    stage("hidden-force", &s);
    for (cell, value) in [("D1", "TRUE"), ("D2", "FALSE"), ("D4", "FALSE")] {
        enter(&mut s, cell, value);
    }
    request(&mut s, SkinIntent::Recalculate);
    stage("toggle-back", &s);
    request(&mut s, SkinIntent::Recalculate);
    stage("back-clean", &s);
    out
}

fn known() -> BTreeSet<String> {
    KNOWN
        .lines()
        .filter(|l| !l.trim().is_empty() && !l.starts_with('#'))
        .map(|l| l.split('\t').next().unwrap().to_string())
        .collect()
}

fn check(backend: calc_vba::Backend) {
    let expected = observations();
    assert_eq!(expected.len(), 17, "stages in observations.json");
    let got = replay(backend);
    let mut different = BTreeSet::new();
    let mut matched = 0usize;
    let mut detail = Vec::new();
    for ((stage, want), (got_stage, have)) in expected.iter().zip(&got) {
        assert_eq!(stage, got_stage);
        for (row, (w, h)) in want.iter().zip(have).enumerate() {
            if w == h {
                matched += 1;
            } else {
                different.insert(format!("{stage}:B{}", row + 1));
                detail.push(format!("{stage}:B{}\texcel={w}\tengine={h}", row + 1));
            }
        }
    }
    eprintln!(
        "{backend:?}: {matched} of {} stage cells match Excel; {} differ",
        matched + different.len(),
        different.len()
    );
    if let Ok(path) = std::env::var("EXCEL_RECALCULATION_DUMP") {
        std::fs::write(path, detail.join("\n") + "\n").unwrap();
    }
    let listed = known();
    let new: Vec<_> = different.difference(&listed).collect();
    let fixed: Vec<_> = listed.difference(&different).collect();
    assert!(
        new.is_empty(),
        "{backend:?}: new differences from Excel: {new:?}"
    );
    assert!(
        fixed.is_empty(),
        "{backend:?}: listed differences now match Excel, remove them: {fixed:?}"
    );
}

#[test]
fn interpreter_replays_excels_recalculation_sequence() {
    check(calc_vba::Backend::Interpreter);
}
