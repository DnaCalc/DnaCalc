use super::*;
use calc_skin::{ConditionalRule, ProviderRequirement, TestCase, TestSuite, TestVerdict, Trial};

fn trial(formula: &str) -> Trial {
    Trial {
        formula: formula.into(),
        culture: "en-US".into(),
        number_format: "0.00".into(),
        now: None,
        inputs: vec![],
        caller: None,
    }
}

#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn formatting_rules_read_raw_values_on_later_pages() {
    let mut host = FormulaHost::default();
    let FormulaAnswer::Evaluated(shown) = host
        .handle(FormulaQuery::Evaluate(trial(
            "=SEQUENCE(40,1,0.001,0.0001)",
        )))
        .unwrap()
    else {
        panic!("missing result")
    };
    let Provenance::Calculated { calc } = shown.provenance else {
        panic!("missing generation")
    };
    let FormulaAnswer::Conditional { result, notes } = host
        .handle(FormulaQuery::ConditionalFormat {
            calc,
            position: ArrayPosition { row: 32, column: 0 },
            rule: ConditionalRule::Expression("=A1>0.00415".into()),
        })
        .unwrap()
    else {
        panic!("missing conditional result")
    };
    let TrialValue::Array { cells, .. } = result.value else {
        panic!("expected array")
    };
    assert_eq!(cells[0][0].formatted, "0.00");
    assert!(cells[0][0].conditional_match);
    assert!(notes[0].contains("8 of 8"));
    let FormulaAnswer::Conditional { result, .. } = host
        .handle(FormulaQuery::ConditionalFormat {
            calc,
            position: ArrayPosition { row: 32, column: 0 },
            rule: ConditionalRule::Expression("=ROW()+COLUMN()=34".into()),
        })
        .unwrap()
    else {
        panic!("expected conditional result")
    };
    let TrialValue::Array { cells, .. } = result.value else {
        panic!("expected array")
    };
    assert!(cells[0][0].conditional_match);
    assert!(!cells[1][0].conditional_match);
}

#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn test_suite_distinguishes_errors_mismatches_and_missing_provider_requirements() {
    let mut host = FormulaHost::default();
    let suite = TestSuite {
        schema: TestSuite::SCHEMA.into(),
        cases: vec![
            TestCase {
                trial: trial("=1/0"),
                expected: "#DIV/0!".into(),
                formatted: None,
            },
            TestCase {
                trial: trial("=1/3"),
                expected: "0.33".into(),
                formatted: Some("0.33".into()),
            },
            TestCase {
                trial: trial("=ROW()"),
                expected: "1".into(),
                formatted: None,
            },
        ],
        providers: vec![],
    };
    let FormulaAnswer::TestResults(outcomes) =
        host.handle(FormulaQuery::RunSuite(suite.clone())).unwrap()
    else {
        panic!("missing suite results")
    };
    assert_eq!(outcomes[0].verdict, TestVerdict::Match);
    assert_eq!(outcomes[0].actual.as_deref(), Some("#DIV/0!"));
    assert_eq!(outcomes[1].verdict, TestVerdict::Mismatch);
    assert!(matches!(
        outcomes[2].verdict,
        TestVerdict::Refused(Refusal::RequiresWorkbook { .. })
    ));
    let mut without_provider = suite;
    without_provider.providers.push(ProviderRequirement {
        name: "Library".into(),
        sha256: "feed".into(),
    });
    assert!(matches!(
        host.handle(FormulaQuery::RunSuite(without_provider)),
        Err(Refusal::Invalid { .. })
    ));
}

#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn workbook_file_exports_formulas_and_named_cells_and_imports_the_authored_values() {
    let mut host = FormulaHost::default();
    let mut authored = trial("=SUM(Figures)+OffsetValue");
    authored.inputs = vec![
        TestInput {
            name: "Figures".into(),
            constant: "{1,2;3,4}".into(),
        },
        TestInput {
            name: "OffsetValue".into(),
            constant: "3".into(),
        },
    ];
    let FormulaAnswer::File(bytes) = host
        .handle(FormulaQuery::ExportWorkbook(authored.clone()))
        .unwrap()
    else {
        panic!("missing workbook")
    };
    let FormulaAnswer::Opened(opened) = host.handle(FormulaQuery::OpenWorkbook(bytes)).unwrap()
    else {
        panic!("missing import")
    };
    assert_eq!(opened.inputs, authored.inputs);
    let FormulaAnswer::Evaluated(shown) = host.handle(FormulaQuery::Evaluate(opened)).unwrap()
    else {
        panic!("missing reopened result")
    };
    let TrialValue::Scalar(cell) = shown.value else {
        panic!("expected scalar")
    };
    assert_eq!(
        cell.raw,
        RawValue::Number {
            bits: 13_f64.to_bits()
        }
    );
}
#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn provider_commands_publish_to_the_trial_and_keep_link_previews_builtin_only() {
    use calc_skin::{Capability, FormulaOperation, ProviderOperation};
    let mut host = FormulaHost::default();
    let reply = host
        .operate(FormulaOperation::Provider(ProviderOperation::LoadVba {
            name: "Module1".into(),
            source:
                "Public Function Twice(ByVal x As Double) As Double\nTwice = x * 2\nEnd Function\n"
                    .into(),
        }))
        .unwrap();
    let FormulaAnswer::Providers(providers) = reply else {
        panic!("missing providers")
    };
    assert!(
        providers
            .functions
            .iter()
            .any(|function| function.name.eq_ignore_ascii_case("Twice"))
    );
    let input = trial("=Twice(3)");
    let FormulaAnswer::Evaluated(preview) =
        host.handle(FormulaQuery::Preview(input.clone())).unwrap()
    else {
        panic!("missing preview")
    };
    let TrialValue::Scalar(preview) = preview.value else {
        panic!("missing scalar")
    };
    assert_eq!(preview.raw.display(), "#NAME?");
    let FormulaAnswer::Evaluated(explicit) = host.handle(FormulaQuery::Evaluate(input)).unwrap()
    else {
        panic!("missing result")
    };
    let TrialValue::Scalar(explicit) = explicit.value else {
        panic!("missing scalar")
    };
    assert_eq!(explicit.raw.display(), "6");
    assert!(matches!(
        host.operate(FormulaOperation::Provider(ProviderOperation::ChooseNative)),
        Err(Refusal::NotAvailableHere {
            capability: Capability::NativeAddins,
            ..
        })
    ));
}
