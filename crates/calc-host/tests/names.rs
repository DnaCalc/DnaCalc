//! dc-cpr: defined names and LET / LAMBDA parameters compare with Excel's Unicode case folding,
//! end to end through calc-host: a name defined through a command, a formula entered as the user
//! types it, the workbook recalculated and the cell projected.
//!
//! The Unicode value expectations are rows of the Excel 16.0 measurement (build 20326, COM, 2026-09-15)
//! recorded in calc-value's `name_case` module, ported from OxFml 15daac5
//! (`crates/oxfml_core/tests/w079_unicode_name_case_tests.rs`): workbook names follow the full
//! case fold (`Größe` = `GRÖSSE` = `GRÖßE` = `GRÖẞE`, final `ς` = `σ`), accents and width stay
//! significant and `İ` / `ı` never equal `I`; LET parameters follow the simple fold, so
//! `=LET(Größe,21,GRÖSSE*2)` reads `GRÖSSE` as the workbook name.
//!
//! The dc-hsa5.6 lifecycle, context and evidence-limit tests are DNA contract controls;
//! they do not claim new Excel measurements.

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

/// Execution evidence, not a new Excel cross-scope measurement or a reusable name value.
#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn dc_hsa5_6__name_invocations_keep_context_versions_and_passive_views() {
    use calc_doc_model::{NameScope, SheetEdit, SheetKey, SheetName};
    use calc_fml::NameResultKind as K;
    use calc_host::{HostSession, SkinIntent};
    for budget in [usize::MAX, 1, 3] {
        let mut book = HostSession::new(open());
        assert!(
            book.dispatch(SkinIntent::SetCalcMode(CalcMode::Manual))
                .accepted()
        );
        let one = book.host().sheets()[0].id();
        assert!(
            book.dispatch(SkinIntent::Edit {
                edits: vec![CellEdit::Sheet(SheetEdit::Insert {
                    key: SheetKey::new(2),
                    name: SheetName::new(calc_host::value::ExcelText::from_interop_assignment(
                        "Other"
                    ))
                    .unwrap(),
                    position: std::num::NonZeroU32::new(2).unwrap(),
                })]
            })
            .accepted()
        );
        let two = book.host().sheets()[1].id();
        let caller = CellKey::new(two, 1, 4).unwrap();
        let closure_caller = CellKey::new(two, 1, 5).unwrap();
        let edits = vec![
            define("Inner", "=$A$1"),
            define("Unused", "=NA()"),
            CellEdit::SetName {
                name: "Outer".into(),
                scope: NameScope::Sheet(one),
                formula: Some("=Inner+1".into()),
            },
            CellEdit::SetName {
                name: "Maker".into(),
                scope: NameScope::Sheet(one),
                formula: Some("=LAMBDA(x,Inner+x)".into()),
            },
            CellEdit::Set {
                at: CellKey::new(one, 1, 1).unwrap(),
                input: interpret_entry("2").unwrap().unwrap(),
            },
            CellEdit::Set {
                at: CellKey::new(two, 1, 1).unwrap(),
                input: interpret_entry("99").unwrap().unwrap(),
            },
            CellEdit::Set {
                at: caller,
                input: interpret_entry("=IF(FALSE,Unused,Sheet1!Outer+Sheet1!Outer)")
                    .unwrap()
                    .unwrap(),
            },
            CellEdit::Set {
                at: closure_caller,
                input: interpret_entry("=LET(fn,Sheet1!Maker,fn(3))")
                    .unwrap()
                    .unwrap(),
            },
        ];
        assert!(book.dispatch(SkinIntent::Edit { edits }).accepted());
        assert!(book.host().name_invocation_view(caller).is_none());
        book.set_calculation_budget(WorkBudget::evaluations(budget));
        assert!(book.dispatch(SkinIntent::Recalculate).accepted());
        while book.has_pending_calculation() {
            assert!(!matches!(
                book.advance_calculation(),
                HostProgress::Failed(_)
            ));
        }
        let table = book.host().model().definitions().name_table();
        let inner = table.lookup(NameScope::Workbook, "Inner").unwrap().id();
        let outer = table.lookup(NameScope::Sheet(one), "Outer").unwrap().id();
        let unused = table.lookup(NameScope::Workbook, "Unused").unwrap().id();
        let before_stats = book.host().plan_cache_stats();
        let view = book.host().name_invocation_view(caller).unwrap();
        assert_eq!(
            book.host().plan_cache_stats(),
            before_stats,
            "inspection neither compiles nor evaluates"
        );
        assert_eq!(view.revision, book.host().revision());
        assert!(
            matches!(view.result.value(), ProjectedValue::Calculated { value, .. } if value.as_number() == Some(6.0))
        );
        assert_eq!(view.invocations.len(), 4);
        assert_eq!(
            view.invocations
                .iter()
                .map(|n| (n.definition.id, n.ordinal, n.parent))
                .collect::<Vec<_>>(),
            vec![
                (inner, 1, Some(0)),
                (outer, 0, None),
                (inner, 3, Some(2)),
                (outer, 2, None)
            ]
        );
        assert!(
            view.invocations
                .iter()
                .all(|n| n.context.caller.cell() == caller
                    && n.context.definition_sheet == Some(one)
                    && n.context.definition == Some(n.definition.id)
                    && n.definition.id != unused)
        );
        assert_eq!(
            view.invocations[0].result,
            K::Reference {
                areas: vec![calc_host::value::GridArea::cell(
                    CellKey::new(one, 1, 1).unwrap()
                )],
                omitted_areas: 0,
            }
        );
        assert_eq!(view.invocations[1].result, K::Number);
        assert_eq!(view.invocations[0].body, view.invocations[2].body);
        assert_eq!(view.invocations[1].body, view.invocations[3].body);
        let closure = book
            .host()
            .name_invocation_view(closure_caller)
            .unwrap_or_else(|| {
                panic!(
                    "closure projection: {:?}",
                    project(
                        book.host(),
                        &ProjectionRequest {
                            cells: vec![closure_caller]
                        }
                    )
                    .unwrap()
                )
            });
        assert_eq!(closure.invocations.len(), 2);
        assert_eq!(closure.invocations[0].result, K::Callable);
        assert_eq!(closure.invocations[1].definition.id, inner);
        // Maker has returned before its closure runs. Parent means active execution,
        // while the compiled definition worksheet remains explicit on Inner.
        assert!(closure.invocations.iter().all(|n| n.parent.is_none()
            && n.context.caller.cell() == closure_caller
            && n.context.definition_sheet == Some(one)));
        assert_ne!(closure.root.locus, view.root.locus);
        assert!(
            matches!(closure.result.value(), ProjectedValue::Calculated { value, .. } if value.as_number() == Some(5.0))
        );
        let old_version = view.invocations[0].definition.version;
        assert!(
            book.dispatch(SkinIntent::Edit {
                edits: vec![define("Inner", "=$A$1*2")]
            })
            .accepted()
        );
        let stale = book.host().name_invocation_view(caller).unwrap();
        assert!(matches!(stale.result.value(), ProjectedValue::Stale(_)));
        assert_eq!(stale.root, view.root);
        assert_eq!(stale.invocations[0].definition.version, old_version);
        assert!(book.host().name_node(inner).unwrap().binding_version() > old_version);
        assert!(book.dispatch(SkinIntent::Recalculate).accepted());
        while book.has_pending_calculation() {
            book.advance_calculation();
        }
        let next = book.host().name_invocation_view(caller).unwrap();
        assert_ne!(next.root.pass.command, view.root.pass.command);
        assert!(
            next.invocations
                .iter()
                .filter(|n| n.definition.id == inner)
                .all(|n| n.definition.version > old_version)
        );
        assert!(
            matches!(next.result.value(), ProjectedValue::Calculated { value, .. } if value.as_number() == Some(10.0))
        );
        assert!(
            book.dispatch(SkinIntent::Edit {
                edits: vec![CellEdit::Set {
                    at: caller,
                    input: interpret_entry("7").unwrap().unwrap(),
                }]
            })
            .accepted()
        );
        assert!(
            book.host().name_invocation_view(caller).is_none(),
            "a literal must not borrow an orphaned formula's evidence"
        );
        assert!(
            book.dispatch(SkinIntent::Edit {
                edits: vec![CellEdit::Clear {
                    area: calc_host::value::GridArea::cell(caller),
                }]
            })
            .accepted()
        );
        assert!(book.host().name_invocation_view(caller).is_none());
    }
}

#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn dc_hsa5_6__name_frames_survive_callback_yields_and_cancel_without_publication() {
    use calc_host::value::CalcValue;
    use calc_host::{HostSession, SkinIntent};
    use std::{cell::Cell, rc::Rc};
    #[derive(Debug)]
    struct Paused {
        release: Rc<Cell<bool>>,
        entries: Rc<Cell<usize>>,
    }
    #[derive(Debug)]
    struct Activation {
        release: Rc<Cell<bool>>,
    }
    impl calc_fml::UserFunction for Paused {
        fn call(&self, _: &[CalcValue]) -> Result<CalcValue, String> {
            panic!("retained callback required");
        }
        fn begin_call(
            &self,
            _: &[CalcValue],
            _: calc_fml::UserInvocationFrame,
            _: &mut dyn calc_fml::UserFunctionContext,
        ) -> Result<Option<Box<dyn calc_fml::UserFunctionInvocation>>, String> {
            self.entries.set(self.entries.get() + 1);
            Ok(Some(Box::new(Activation {
                release: self.release.clone(),
            })))
        }
    }
    impl calc_fml::UserFunctionInvocation for Activation {
        fn resume(
            &mut self,
            _: &mut dyn calc_fml::UserFunctionContext,
            _: u64,
        ) -> Result<calc_fml::UserFunctionStep, String> {
            Ok(if self.release.get() {
                calc_fml::UserFunctionStep::Complete(CalcValue::number(2.0))
            } else {
                calc_fml::UserFunctionStep::Suspended
            })
        }
    }
    for budget in [1, 3] {
        for cancel in [false, true] {
            let mut book = HostSession::new(open());
            book.set_calculation_budget(WorkBudget::evaluations(budget));
            let release = Rc::new(Cell::new(false));
            let entries = Rc::new(Cell::new(0));
            let mut functions = calc_fml::CallableTable::new();
            functions
                .register(
                    "Paused",
                    Rc::new(Paused {
                        release: release.clone(),
                        entries: entries.clone(),
                    }),
                )
                .unwrap();
            book.set_user_functions(functions);
            let caller = CellKey::new(book.host().sheets()[0].id(), 1, 1).unwrap();
            assert!(
                book.dispatch(SkinIntent::Edit {
                    edits: vec![
                        define("Inner", "=Paused(2)"),
                        define("Outer", "=Inner+1"),
                        CellEdit::Set {
                            at: caller,
                            input: interpret_entry("=Outer+Outer").unwrap().unwrap()
                        }
                    ]
                })
                .accepted()
            );
            for _ in 0..20 {
                if entries.get() != 0 {
                    break;
                }
                book.advance_calculation();
            }
            assert_eq!(entries.get(), 1);
            for _ in 0..3 {
                book.advance_calculation();
            }
            assert_eq!(
                entries.get(),
                1,
                "same retained name and callback activation"
            );
            assert!(
                book.host().name_invocation_view(caller).is_none(),
                "no incomplete name execution is installed"
            );
            let first_command = book
                .take_calculation_events()
                .into_iter()
                .find_map(|event| {
                    if let calc_host::CalcEvent::Admitted(admission) = event {
                        Some(admission.command)
                    } else {
                        None
                    }
                })
                .unwrap();
            if cancel {
                book.cancel_calculation();
                assert!(book.host().name_invocation_view(caller).is_none());
            }
            release.set(true);
            if cancel {
                assert!(book.dispatch(SkinIntent::Recalculate).accepted());
            }
            while book.has_pending_calculation() {
                assert!(!matches!(
                    book.advance_calculation(),
                    HostProgress::Failed(_)
                ));
            }
            assert_eq!(entries.get(), if cancel { 3 } else { 2 });
            let view = book.host().name_invocation_view(caller).unwrap();
            assert_eq!(
                view.invocations
                    .iter()
                    .map(|n| (n.ordinal, n.parent))
                    .collect::<Vec<_>>(),
                vec![(1, Some(0)), (0, None), (3, Some(2)), (2, None)]
            );
            assert_eq!(view.root.pass.command != first_command, cancel);
            assert!(
                matches!(view.result.value(), ProjectedValue::Calculated { value, .. } if value.as_number() == Some(6.0))
            );
        }
    }
}

#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn dc_hsa5_6__refused_name_bodies_keep_fault_context_without_a_value() {
    use calc_host::value::CalcValue;
    use calc_host::{HostSession, SkinIntent};
    #[derive(Debug)]
    struct Refuses;
    impl calc_fml::UserFunction for Refuses {
        fn call(&self, _: &[CalcValue]) -> Result<CalcValue, String> {
            panic!("context required");
        }
        fn call_with_context(
            &self,
            _: &[CalcValue],
            context: &mut dyn calc_fml::UserFunctionContext,
        ) -> Result<CalcValue, String> {
            Err(context.refuse_by("a refused named-body control", "dc-hsa5.6"))
        }
    }
    let mut book = HostSession::new(open());
    let mut functions = calc_fml::CallableTable::new();
    functions
        .register("Refuses", std::rc::Rc::new(Refuses))
        .unwrap();
    book.set_user_functions(functions);
    let caller = CellKey::new(book.host().sheets()[0].id(), 1, 1).unwrap();
    assert!(
        book.dispatch(SkinIntent::Edit {
            edits: vec![
                define("Inner", "=Refuses()"),
                define("Outer", "=Inner+1"),
                CellEdit::Set {
                    at: caller,
                    input: interpret_entry("=Outer").unwrap().unwrap()
                }
            ]
        })
        .accepted()
    );
    while book.has_pending_calculation() {
        book.advance_calculation();
    }
    let view = book.host().name_invocation_view(caller).unwrap();
    assert!(
        matches!(view.result.value(), ProjectedValue::NotModeled { refusal, .. } if refusal.to_string().contains("dc-hsa5.6"))
    );
    assert_eq!(
        view.invocations
            .iter()
            .map(|n| (n.ordinal, n.parent, &n.result))
            .collect::<Vec<_>>(),
        vec![
            (1, Some(0), &calc_fml::NameResultKind::Refused),
            (0, None, &calc_fml::NameResultKind::Refused),
        ]
    );
}

#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn dc_hsa5_6__bounded_name_evidence_counts_omissions_without_shortening_calculation() {
    use calc_host::{HostSession, SkinIntent};
    let count = calc_fml::MAX_RECORDED_NAME_INVOCATIONS + 7;
    let union = [
        "B", "D", "F", "H", "J", "L", "N", "P", "R", "T", "V", "X", "Z", "AB", "AD", "AF", "AH",
        "AJ", "AL", "AN",
    ]
    .map(|column| format!("${column}$1"))
    .join(",");
    for budget in [1, usize::MAX] {
        let mut book = HostSession::new(open());
        book.set_calculation_budget(WorkBudget::evaluations(budget));
        let sheet = book.host().sheets()[0].id();
        let a1 = CellKey::new(sheet, 1, 1).unwrap();
        let a2 = CellKey::new(sheet, 2, 1).unwrap();
        assert!(
            book.dispatch(SkinIntent::Edit {
                edits: vec![
                    define("Leaf", "=1+0"),
                    define("Union", &format!("=({union})")),
                    CellEdit::Set {
                        at: a1,
                        input: interpret_entry(&format!(
                            "=SUM(MAP(SEQUENCE({count}),LAMBDA(x,Leaf)))"
                        ))
                        .unwrap()
                        .unwrap()
                    },
                    CellEdit::Set {
                        at: a2,
                        input: interpret_entry("=AREAS(Union)").unwrap().unwrap()
                    },
                ]
            })
            .accepted()
        );
        while book.has_pending_calculation() {
            assert!(!matches!(
                book.advance_calculation(),
                HostProgress::Failed(_)
            ));
        }
        let many = book.host().name_invocation_view(a1).unwrap();
        assert_eq!(
            many.invocations.len(),
            calc_fml::MAX_RECORDED_NAME_INVOCATIONS
        );
        assert_eq!(many.omitted, 7);
        assert!(
            matches!(many.result.value(), ProjectedValue::Calculated { value, .. } if value.as_number() == Some(count as f64))
        );
        let reference = book.host().name_invocation_view(a2).unwrap();
        assert_eq!(reference.omitted, 0);
        assert_eq!(reference.invocations.len(), 1);
        let calc_fml::NameResultKind::Reference {
            areas,
            omitted_areas,
        } = &reference.invocations[0].result
        else {
            panic!("reference body");
        };
        assert_eq!(areas.len(), 16);
        assert_eq!(*omitted_areas, 4);
        assert!(
            matches!(reference.result.value(), ProjectedValue::Calculated { value, .. } if value.as_number() == Some(20.0))
        );
    }
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

#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn names_memo_candidates_follow_transitive_edits_history_and_keep_inspection_passive() {
    use calc_fml::{NameMemoBlocker as B, NameMemoEligibility as E};
    use calc_host::{HostSession, SkinIntent};
    #[derive(Debug)]
    struct CountedMirror(std::rc::Rc<std::cell::Cell<usize>>);
    impl calc_fml::UserFunction for CountedMirror {
        fn permits_early_execution(&self) -> bool {
            true
        }
        fn call(
            &self,
            args: &[calc_host::value::CalcValue],
        ) -> Result<calc_host::value::CalcValue, String> {
            self.0.set(self.0.get() + 1);
            args.first()
                .cloned()
                .ok_or_else(|| "missing mirror argument".into())
        }
    }
    let calls = std::rc::Rc::new(std::cell::Cell::new(0));
    let mut book = HostSession::new(open());
    let mut functions = calc_fml::CallableTable::new();
    functions
        .register(
            "CountedMirror",
            std::rc::Rc::new(CountedMirror(calls.clone())),
        )
        .unwrap();
    book.set_user_functions(functions);
    let sheet = book.host().sheets()[0].id();
    let at = |col| CellKey::new(sheet, 1, col).unwrap();
    let mut edits = vec![
        define("Inner", "=2"),
        define("Outer", "=Inner+1"),
        define("Reference", "=$D$1+1"),
        define("Random", "=RAND()"),
        define("Branch", "=IF(TRUE,1,Random)"),
        define("Closure", "=LAMBDA(x,x+1)"),
        define("Facility", "=SUM(1,2)"),
        define("Locals", "=LET(x,1,x+1)"),
        define("Pure", "=3"),
        define("Unused", "=RAND()"),
        define("Loop", "=Loop"),
        define("UserBody", "=CountedMirror(2)"),
    ];
    for (col, text) in [
        (1, "=Outer+Outer"),
        (2, "=2+2"),
        (3, "=Reference"),
        (4, "10"),
        (5, "=Branch"),
        (6, "=Closure"),
        (7, "=Facility"),
        (8, "=Locals"),
        (9, "=Random+Pure"),
        (10, "=IF(FALSE,Loop,1)"),
        (11, "=UserBody"),
    ] {
        edits.push(CellEdit::Set {
            at: at(col),
            input: interpret_entry(text).unwrap().unwrap(),
        });
    }
    assert!(book.host().name_memo_candidates(at(1)).is_none());
    assert!(book.dispatch(SkinIntent::Edit { edits }).accepted());
    while book.has_pending_calculation() {
        book.advance_calculation();
    }
    let id = |name| {
        book.host()
            .model()
            .definitions()
            .name_table()
            .lookup(calc_doc_model::NameScope::Workbook, name)
            .unwrap()
            .id()
    };
    let outer = id("Outer");
    let pure = id("Pure");
    let random = id("Random");
    let unused = id("Unused");
    let initial = book.host().name_memo_candidates(at(1)).unwrap();
    assert_eq!(initial.iter().filter(|c| c.name == outer).count(), 1);
    assert!(
        initial
            .iter()
            .all(|c| c.eligibility == E::RequiresReadCertificate { cell_reads: false })
    );
    let old = Arc::downgrade(&initial[0].plan);
    assert_eq!(
        book.host().name_memo_candidates(at(3)).unwrap()[0].eligibility,
        E::RequiresReadCertificate { cell_reads: true }
    );
    for (col, reason) in [
        (5, B::UnclassifiedFunction),
        (6, B::CallableValue),
        (7, B::UnclassifiedFunction),
        (8, B::LocalEnvironment),
        (10, B::RecursiveDefinition),
        (11, B::UserFunction),
    ] {
        assert!(
            book.host()
                .name_memo_candidates(at(col))
                .unwrap()
                .iter()
                .any(|c| c.eligibility == E::Ineligible(reason)),
            "column {col}"
        );
    }
    let mixed = book.host().name_memo_candidates(at(9)).unwrap();
    assert_eq!(
        mixed.iter().find(|c| c.name == pure).unwrap().eligibility,
        E::RequiresReadCertificate { cell_reads: false }
    );
    assert_eq!(
        mixed.iter().find(|c| c.name == random).unwrap().eligibility,
        E::Ineligible(B::Volatile)
    );
    assert!(!mixed.iter().any(|c| c.name == unused));
    drop(mixed);
    let unrelated = project(book.host(), &ProjectionRequest { cells: vec![at(2)] }).unwrap();
    let baseline_live = book.host().plan_cache_stats().live;
    assert!(
        book.dispatch(SkinIntent::SetCalcMode(CalcMode::Manual))
            .accepted()
    );
    for (intent, volatile) in [
        (
            SkinIntent::Edit {
                edits: vec![define("Inner", "=RAND()")],
            },
            true,
        ),
        (SkinIntent::Undo, false),
        (SkinIntent::Redo, true),
    ] {
        assert!(book.dispatch(intent).accepted());
        assert!(book.host().name_memo_candidates(at(1)).is_none());
        assert!(book.dispatch(SkinIntent::Recalculate).accepted());
        while book.has_pending_calculation() {
            book.advance_calculation();
        }
        let candidates = book.host().name_memo_candidates(at(1)).unwrap();
        let candidate = candidates.iter().find(|c| c.name == outer).unwrap();
        assert_eq!(
            candidate.eligibility,
            if volatile {
                E::Ineligible(B::Volatile)
            } else {
                E::RequiresReadCertificate { cell_reads: false }
            }
        );
        assert!(
            old.upgrade().is_some(),
            "an explicitly retained analysis owns its old plan"
        );
    }
    drop(initial);
    assert!(old.upgrade().is_none());
    for version in 0..256 {
        assert!(
            book.dispatch(SkinIntent::Edit {
                edits: vec![define("Inner", &format!("={version}"))]
            })
            .accepted()
        );
        assert!(book.dispatch(SkinIntent::Recalculate).accepted());
        while book.has_pending_calculation() {
            book.advance_calculation();
        }
        assert_eq!(book.host().plan_cache_stats().live, baseline_live);
    }
    let stats = book.host().plan_cache_stats();
    assert!(stats.entries <= baseline_live + 4, "{stats:?}");
    assert_eq!(
        project(book.host(), &ProjectionRequest { cells: vec![at(2)] })
            .unwrap()
            .cells()[0]
            .value(),
        unrelated.cells()[0].value()
    );
    assert_eq!(calls.get(), 1);
    let before = book.host().plan_cache_stats();
    for col in 1..=11 {
        let _ = book.host().name_memo_candidates(at(col));
    }
    assert_eq!(book.host().plan_cache_stats(), before);
    assert!(!book.has_pending_calculation());
    assert_eq!(calls.get(), 1);
}
