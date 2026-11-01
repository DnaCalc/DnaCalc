//! dc-cpr: defined names and LET / LAMBDA parameters compare with Excel's Unicode case folding,
//! end to end through calc-host: a name defined through a command, a formula entered as the user
//! types it, the workbook recalculated and the cell projected.
//!
//! Every expectation is a row of the Excel 16.0 measurement (build 20326, COM, 2026-09-15)
//! recorded in calc-value's `name_case` module, ported from OxFml 15daac5
//! (`crates/oxfml_core/tests/w079_unicode_name_case_tests.rs`): workbook names follow the full
//! case fold (`Größe` = `GRÖSSE` = `GRÖßE` = `GRÖẞE`, final `ς` = `σ`), accents and width stay
//! significant and `İ` / `ı` never equal `I`; LET parameters follow the simple fold, so
//! `=LET(Größe,21,GRÖSSE*2)` reads `GRÖSSE` as the workbook name.

// Case names are fixed identities of the form `<bead>__<case>`.
#![allow(non_snake_case)]

use calc_host::value::{CellKey, WorksheetErrorCode};
use calc_host::{
    CalcMode, CellEdit, HostCommand, HostOptions, HostProgress, HostState, MemorySource,
    OpenPolicy, PackageRead, ProjectedValue, ProjectionRequest, Unobserved, WorkBudget, dispatch,
    interpret_entry, project, step,
};
use std::sync::Arc;

fn open() -> HostState {
    let source: Arc<dyn PackageRead> = Arc::new(MemorySource::new(calc_host::blank_package()));
    HostState::open(
        source,
        &OpenPolicy::default(),
        HostOptions {
            calc_mode: Some(CalcMode::Automatic),
            ..HostOptions::default()
        },
    )
    .expect("a new workbook opens")
}

fn commit(host: &mut HostState, edits: Vec<CellEdit>) {
    let base = host.revision();
    dispatch(host, HostCommand { base, edits }).expect("the command commits");
}

/// The workbook name `name` defined as `formula`.
fn define(name: &str, formula: &str) -> CellEdit {
    CellEdit::SetName {
        name: name.to_owned(),
        scope: calc_doc_model::NameScope::Workbook,
        formula: Some(formula.to_owned()),
    }
}

/// What A1 shows after `entry` is typed into it with `names` defined, as a number or an error.
fn shown(names: &[(&str, &str)], entry: &str) -> String {
    let mut host = open();
    let sheet = host.sheets()[0].id();
    let a1 = CellKey::new(sheet, 1, 1).expect("A1");
    let mut edits: Vec<CellEdit> = names.iter().map(|(name, f)| define(name, f)).collect();
    let input = interpret_entry(entry)
        .unwrap_or_else(|e| panic!("the entry {entry:?}: {e:?}"))
        .expect("a formula");
    edits.push(CellEdit::Set { at: a1, input });
    commit(&mut host, edits);
    loop {
        match step(&mut host, WorkBudget::UNLIMITED, &mut Unobserved) {
            HostProgress::Calculating { .. } => {}
            _ => break,
        }
    }
    let projection = project(&host, &ProjectionRequest { cells: vec![a1] }).expect("projected");
    match projection.cells()[0].value() {
        ProjectedValue::Calculated { value, .. } => match value.core_ref() {
            calc_host::value::CoreRef::Number(n) => format!("{n}"),
            calc_host::value::CoreRef::Error(code) => format!("{code:?}"),
            other => format!("{other:?}"),
        },
        other => format!("{other:?}"),
    }
}

fn name_error() -> String {
    format!("{:?}", WorksheetErrorCode::Name)
}

#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn dc_cpr__workbook_names_resolve_under_the_full_fold_across_scripts() {
    for (defined, references) in [
        ("Total", &["Total", "TOTAL", "total", "tOtAl"][..]),
        ("Größe", &["GRÖSSE", "GRÖßE", "größe", "GRÖ\u{1E9E}E"][..]),
        ("Strasse", &["Straße", "Stra\u{1E9E}E", "STRASSE"][..]),
        ("Μέγεθος", &["ΜΈΓΕΘΟΣ", "μέγεθοσ", "μέγεθος"][..]),
        ("Данные", &["ДАННЫЕ", "данные"][..]),
        ("Année", &["ANNÉE", "année"][..]),
        ("売上", &["売上"][..]),
    ] {
        for reference in references {
            assert_eq!(
                shown(&[(defined, "=21")], &format!("={reference}*2")),
                "42",
                "`{reference}` should resolve the name `{defined}`"
            );
        }
    }
}

#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn dc_cpr__accents_width_and_turkic_i_stay_significant() {
    for (defined, reference) in [
        ("Μέγεθος", "ΜΕΓΕΘΟΣ"),
        ("Wide", "\u{FF37}ide"),
        ("Isik", "\u{0130}sik"),
        ("ISIK", "\u{0131}SIK"),
    ] {
        assert_eq!(
            shown(&[(defined, "=21")], &format!("={reference}*2")),
            name_error(),
            "`{reference}` is not the name `{defined}`"
        );
    }
}

#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn dc_cpr__let_parameters_follow_the_simple_fold() {
    // A parameter matched under the simple fold: 42.
    for formula in [
        "=LET(Größe,21,GRÖßE*2)",
        "=LET(Größe,21,größe*2)",
        "=LET(Straße,21,Stra\u{1E9E}E*2)",
        "=LET(Sigmaς,21,SIGMAΣ*2)",
        "=LET(Sigmaς,21,sigmaσ*2)",
        "=LET(Æther,21,ÆTHER*2)",
    ] {
        assert_eq!(shown(&[], formula), "42", "{formula}");
    }
    // Not the parameter under the simple fold: the workbook name of that spelling (99) takes it,
    // as Excel answered 198 with the name defined.
    assert_eq!(shown(&[("Größe", "=99")], "=LET(Größe,21,GRÖSSE*2)"), "198");
    assert_eq!(
        shown(&[("Strasse", "=99")], "=LET(Straße,21,STRASSE*2)"),
        "198"
    );
    // Neither a parameter nor a defined name: #NAME?.
    assert_eq!(shown(&[], "=LET(xı,21,XI*2)"), name_error());
    assert_eq!(shown(&[], "=LET(x\u{0130},21,xi*2)"), name_error());
}

#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn dc_cpr__a_case_variant_redefines_the_one_name() {
    // Excel kept one name when `Names.Add` was given a case variant (the count unchanged): the
    // variant gives the existing name its formula.
    let mut host = open();
    commit(
        &mut host,
        vec![define("Größe", "=21"), define("GRÖSSE", "=50")],
    );
    let names: Vec<_> = host
        .model()
        .definitions()
        .names()
        .iter()
        .map(|name| (name.name.clone(), name.formula.clone()))
        .collect();
    assert_eq!(names, vec![("Größe".to_owned(), "=50".to_owned())]);
}

#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn names_product_edit_undo_redo_rebind_without_reauthoring_cells() {
    use calc_host::{HostSession, SkinIntent};
    let mut book = HostSession::new(open());
    let sheet = book.host().sheets()[0].id();
    let at = |col| CellKey::new(sheet, 1, col).unwrap();
    let input = |text| interpret_entry(text).unwrap().unwrap();
    assert!(
        book.dispatch(SkinIntent::Edit {
            edits: vec![
                define("Rate", "=2"),
                CellEdit::Set {
                    at: at(1),
                    input: input("=Rate+1")
                },
                CellEdit::Set {
                    at: at(2),
                    input: input("=A1+1")
                },
                CellEdit::Set {
                    at: at(3),
                    input: input("=2+2")
                },
            ]
        })
        .accepted()
    );
    while book.has_pending_calculation() {
        book.advance_calculation();
    }
    let original = book
        .host()
        .model()
        .definitions()
        .name_table()
        .lookup(calc_doc_model::NameScope::Workbook, "Rate")
        .unwrap()
        .id();
    let unchanged = project(book.host(), &ProjectionRequest { cells: vec![at(3)] }).unwrap();
    for (intent, expected) in [
        (
            SkinIntent::Edit {
                edits: vec![define("Rate", "=7")],
            },
            8.0,
        ),
        (SkinIntent::Undo, 3.0),
        (SkinIntent::Redo, 8.0),
    ] {
        assert!(book.dispatch(intent).accepted());
        while book.has_pending_calculation() {
            book.advance_calculation();
        }
        let projection = project(
            book.host(),
            &ProjectionRequest {
                cells: vec![at(1), at(2), at(3)],
            },
        )
        .unwrap();
        for (cell, expected) in projection
            .cells()
            .iter()
            .zip([expected, expected + 1.0, 4.0])
        {
            assert!(
                matches!(cell.value(), ProjectedValue::Calculated { value, .. } if *value == calc_host::value::CalcValue::number(expected)),
                "expected {expected}, got {:?}",
                cell.value()
            );
        }
        assert_eq!(projection.cells()[2].value(), unchanged.cells()[0].value());
        assert_eq!(
            book.host()
                .model()
                .definitions()
                .name_table()
                .lookup(calc_doc_model::NameScope::Workbook, "Rate")
                .unwrap()
                .id(),
            original
        );
    }
    assert!(
        book.dispatch(SkinIntent::Edit {
            edits: vec![CellEdit::SetName {
                name: "Rate".into(),
                scope: calc_doc_model::NameScope::Workbook,
                formula: None,
            }]
        })
        .accepted()
    );
    assert!(book.dispatch(SkinIntent::Undo).accepted());
    while book.has_pending_calculation() {
        book.advance_calculation();
    }
    let restored = book
        .host()
        .model()
        .definitions()
        .name_table()
        .lookup(calc_doc_model::NameScope::Workbook, "Rate")
        .unwrap()
        .id();
    assert_ne!(original, restored);
    assert!(
        book.host()
            .model()
            .definitions()
            .name_table()
            .get(original)
            .is_none()
    );
}

/// Native-only: product dispatch and history wall-clock cost, calculation outside the timer.
#[cfg(not(target_arch = "wasm32"))]
#[test]
#[ignore = "run explicitly for the name edit measurements"]
fn names_product_edit_cost_measurement() {
    use calc_host::{HostSession, SkinIntent};
    use std::time::Instant;
    for count in [1_000usize, 100_000] {
        let mut host = open();
        commit(
            &mut host,
            (0..count)
                .map(|i| define(&format!("Name_{i:06}"), "=1"))
                .collect(),
        );
        let sheet = host.sheets()[0].id();
        let at = |col| CellKey::new(sheet, 1, col).unwrap();
        let name = format!("Name_{:06}", count - 1);
        let mut book = HostSession::new(host);
        assert!(
            book.dispatch(SkinIntent::Edit {
                edits: vec![
                    CellEdit::Set {
                        at: at(1),
                        input: interpret_entry(&format!("={name}+1")).unwrap().unwrap()
                    },
                    CellEdit::Set {
                        at: at(2),
                        input: interpret_entry("=A1+1").unwrap().unwrap()
                    },
                    CellEdit::Set {
                        at: at(3),
                        input: interpret_entry("=2+2").unwrap().unwrap()
                    },
                ]
            })
            .accepted()
        );
        while book.has_pending_calculation() {
            book.advance_calculation();
        }
        assert!(
            book.dispatch(SkinIntent::SetCalcMode(CalcMode::Manual))
                .accepted()
        );
        let mut times = Vec::new();
        for value in 2..=12 {
            let start = Instant::now();
            let receipt = book.dispatch(SkinIntent::Edit {
                edits: vec![define(&name, &format!("={value}"))],
            });
            times.push(start.elapsed().as_secs_f64() * 1e6);
            assert!(receipt.accepted());
            assert!(book.dispatch(SkinIntent::Recalculate).accepted());
            while book.has_pending_calculation() {
                book.advance_calculation();
            }
            let projection =
                project(book.host(), &ProjectionRequest { cells: vec![at(1)] }).unwrap();
            assert!(
                matches!(projection.cells()[0].value(), ProjectedValue::Calculated { value: actual, .. } if *actual == calc_host::value::CalcValue::number(f64::from(value+1)))
            );
        }
        times.sort_by(f64::total_cmp);
        println!(
            "NAME-PRODUCT-COST profile=debug names={count} formulas=3 edits=11 history=retained median_us={:.3} min_us={:.3} max_us={:.3}",
            times[5], times[0], times[10]
        );
        assert!(book.dispatch(SkinIntent::Undo).accepted());
        assert!(book.dispatch(SkinIntent::Recalculate).accepted());
        while book.has_pending_calculation() {
            book.advance_calculation();
        }
        let projection = project(book.host(), &ProjectionRequest { cells: vec![at(1)] }).unwrap();
        assert!(
            matches!(projection.cells()[0].value(), ProjectedValue::Calculated { value, .. } if *value == calc_host::value::CalcValue::number(12.0))
        );
    }
}

#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn names_qualified_function_spellings_keep_their_defining_sheet_when_authored() {
    use calc_doc_model::{SheetEdit, SheetKey, SheetName};
    use calc_host::{HostSession, SkinIntent};
    let mut book = HostSession::new(open());
    let one = book.host().sheets()[0].id();
    assert!(
        book.dispatch(SkinIntent::Edit {
            edits: vec![CellEdit::Sheet(SheetEdit::Insert {
                key: SheetKey::new(2),
                name: SheetName::new(calc_host::value::ExcelText::from_utf16_code_units(
                    "Other sheet".encode_utf16().collect()
                ))
                .unwrap(),
                position: std::num::NonZeroU32::new(2).unwrap(),
            })]
        })
        .accepted()
    );
    let two = book.host().sheets()[1].id();
    let a1 = CellKey::new(two, 1, 1).unwrap();
    let b1 = CellKey::new(two, 1, 2).unwrap();
    assert!(
        book.dispatch(SkinIntent::Edit {
            edits: vec![
                CellEdit::SetName {
                    name: "Rate".into(),
                    scope: calc_doc_model::NameScope::Sheet(one),
                    formula: Some("7".into()),
                },
                CellEdit::Set {
                    at: a1,
                    input: interpret_entry("=Sheet1!Rate+1").unwrap().unwrap()
                },
                CellEdit::Set {
                    at: CellKey::new(one, 1, 1).unwrap(),
                    input: interpret_entry("6").unwrap().unwrap()
                },
                CellEdit::Set {
                    at: b1,
                    input: interpret_entry("=Sheet1!A1+1").unwrap().unwrap()
                },
            ]
        })
        .accepted()
    );
    while book.has_pending_calculation() {
        book.advance_calculation();
    }
    let projection = project(book.host(), &ProjectionRequest { cells: vec![a1] }).unwrap();
    assert!(
        matches!(projection.cells()[0].value(),ProjectedValue::Calculated {value,..} if *value==calc_host::value::CalcValue::number(8.0))
    );
    let calc_doc_model::CellInput::Formula(text) = book.host().model().cell(a1).unwrap() else {
        panic!("a formula")
    };
    assert_eq!(text.text().to_string_lossy(), "Sheet1!Rate+1");
    assert!(
        book.host().model().definitions().array(b1).is_none(),
        "a qualified scalar cell keeps ordinary formula form"
    );
    let projection = project(book.host(), &ProjectionRequest { cells: vec![b1] }).unwrap();
    assert!(
        matches!(projection.cells()[0].value(), ProjectedValue::Calculated {value,..} if *value==calc_host::value::CalcValue::number(7.0))
    );
    let mut saved = Vec::new();
    calc_host::save(book.host(), &calc_host::SavePolicy::default(), &mut saved).unwrap();
    let reopened = HostState::open(
        Arc::new(MemorySource::new(saved)),
        &OpenPolicy::default(),
        HostOptions::default(),
    )
    .unwrap();
    let reopened_a1 = CellKey::new(reopened.sheets()[1].id(), 1, 1).unwrap();
    let calc_doc_model::CellInput::Formula(text) = reopened.model().cell(reopened_a1).unwrap()
    else {
        panic!("a reopened formula")
    };
    assert_eq!(text.text().to_string_lossy(), "Sheet1!Rate+1");
    assert!(
        reopened
            .model()
            .definitions()
            .array(CellKey::new(reopened.sheets()[1].id(), 1, 2).unwrap())
            .is_none()
    );
}
