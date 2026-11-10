#![cfg(windows)]
//! DNA OneCalc's desktop executable: native add-ins through the formula session, no engine. The same case files run through DNA Calc's `--engine` harness (`dna-calc-desktop`'s tests).
#[path = "../../dna-calc-desktop/tests/support/mod.rs"]
mod support;
use support::{Fixture, checked};

fn new_fixture() -> Fixture {
    Fixture::new(std::path::Path::new(env!(
        "CARGO_BIN_EXE_dna-onecalc-desktop"
    )))
}
/// Native-only: loads a native XLL in the exported Windows executable and calculates formulas.
#[test]
fn real_executable_uses_onecalc_for_the_same_excel_fixture() {
    let fixture = new_fixture();
    let result = checked(fixture.command().arg("--self-test").output().unwrap());
    assert_eq!(result["results"].as_array().unwrap().len(), 289);
    assert_eq!(result["results"][0]["value"], 3.);
    assert_eq!(result["results"][16]["value"], 41.);
    assert_eq!(result["results"][17]["value"], 42.);
    assert_eq!(result["duplicate_use_count"], 2);
    assert_eq!(result["live_callback_allocations"], 0);
    assert_eq!(result["closed_registration"], "#NAME?");
    // The executable bound its own companion by full path before the first XLL (A.3 #5).
    let companion = std::path::PathBuf::from(result["companion"].as_str().unwrap());
    assert_eq!(
        std::fs::canonicalize(&companion).unwrap(),
        std::fs::canonicalize(fixture.directory.join("XLCALL32.DLL")).unwrap()
    );
}

/// Native-only: invokes typed ABI entries through the application and real XLL.
#[test]
fn typed_native_arguments_coerce_values_and_return_worksheet_errors() {
    let fixture = new_fixture();
    for (formula, expected) in [
        ("=MyAdd(\"19\",23)", serde_json::json!(42.)),
        ("=MyAdd(TRUE,41)", serde_json::json!(42.)),
        (
            "=MyAdd(\"invalid\",23)",
            serde_json::json!({"error":"#VALUE!"}),
        ),
        ("=MyAdd(NA(),23)", serde_json::json!({"error":"#VALUE!"})),
        ("=MyWide(12.25)", serde_json::json!("12.25")),
        ("=MyWide(TRUE)", serde_json::json!("TRUE")),
        ("=MyWide()", serde_json::json!("")),
    ] {
        let result = checked(
            fixture
                .command()
                .args(["--formula", formula])
                .output()
                .unwrap(),
        );
        assert_eq!(result["results"][0]["value"], expected, "{formula}");
        assert_eq!(result["live_callback_allocations"], 0);
    }
}

/// Native-only: REFTEXT runs inside a real macro-equivalent XLL call.
#[test]
fn native_reference_text_uses_the_existing_address_formatter() {
    let fixture = new_fixture();
    let result = checked(
        fixture
            .command()
            .env("DNA_XLL_REFERENCE_TEXT", "1")
            .args(["--sheet", "Q'2 sales", "--formula", "=MyReference(A1:A2)"])
            .output()
            .unwrap(),
    );
    assert_eq!(
        result["results"][0]["value"],
        "'[Book1]Q''2 sales'!$A$1:$A$2"
    );
    assert_eq!(result["live_callback_allocations"], 0);
}

/// Native-only: an unsupported locale-dependent input must survive IFERROR.
#[test]
fn unmodelled_native_argument_conversion_cannot_be_hidden() {
    let fixture = new_fixture();
    let output = fixture
        .command()
        .args(["--formula", "=IFERROR(MyAdd(\"12/15\",1),123)"])
        .output()
        .unwrap();
    assert!(!output.status.success());
    let error = String::from_utf8_lossy(&output.stderr);
    assert!(
        error.contains("NotModelled") && error.contains("dc-snz.20"),
        "{error}"
    );
    assert!(output.stdout.is_empty());
}

/// Native-only: closing the host enters only xlAutoClose, as Excel's UNREGISTER does (X2 traces);
/// xlAutoRemove is the Add-in Manager's removal action and is not entered.
#[test]
fn closing_enters_only_xl_auto_close() {
    let fixture = new_fixture();
    let log = fixture.directory.join("lifecycle.log");
    let result = checked(
        fixture
            .command()
            .env("DNA_XLL_LIFECYCLE_LOG", &log)
            .args(["--formula", "=MyAdd(1,2)"])
            .output()
            .unwrap(),
    );
    assert_eq!(result["results"][0]["value"], 3.);
    assert_eq!(
        std::fs::read_to_string(&log).unwrap(),
        "xlAutoOpen\nxlAutoClose\n"
    );
}

/// Native-only: the x13 boundary batch (dc-6332.10) through OneCalc's executable. Every case
/// the host models gives Excel 16.0.20527's recorded answer (`x13-boundary-cases.json`: callback
/// status, thread, #VALUE! in failed result operands, xlFree's operand change, xlGetInst 32,
/// xlStack's reserve, callback storage kept across entries); the exe checks each value. The
/// three it does not model stay typed refusals.
#[test]
fn boundary_probes_give_excel_answers() {
    const REFUSED: [&str; 3] = [
        "=MyBoundaryMacro(32909)",
        "=MyBoundaryMacro(255)",
        "=MyBoundaryPool(16401)",
    ];
    let cases: Vec<serde_json::Value> = serde_json::from_str(include_str!(
        "../../../crates/calc-xll-reference/fixture/x13-boundary-cases.json"
    ))
    .unwrap();
    let served = cases
        .iter()
        .filter(|case| !REFUSED.iter().any(|formula| case["formula"] == *formula))
        .cloned()
        .collect::<Vec<_>>();
    assert_eq!(served.len(), cases.len() - REFUSED.len());
    assert!(served.iter().all(|case| !case["value"].is_null()));
    for (cases, open_inst) in [
        (served, false),
        (
            serde_json::from_str(include_str!(
                "../../../crates/calc-xll-reference/fixture/x13-boundary-open-cases.json"
            ))
            .unwrap(),
            true,
        ),
    ] {
        let fixture = new_fixture();
        let path = fixture.directory.join("boundary.json");
        std::fs::write(&path, serde_json::to_vec(&cases).unwrap()).unwrap();
        let mut command = fixture.command();
        command.env("DNA_XLL_BOUNDARY_TESTS", "1");
        if open_inst {
            command.env("DNA_XLL_BOUNDARY_OPEN_INST", "1");
        }
        let result = checked(
            command
                .args(["--sheet", "X0", "--cases"])
                .arg(&path)
                .output()
                .unwrap(),
        );
        assert_eq!(result["results"].as_array().unwrap().len(), cases.len());
        assert_eq!(result["live_callback_allocations"], 0);
    }
    for formula in REFUSED {
        let fixture = new_fixture();
        let output = fixture
            .command()
            .env("DNA_XLL_BOUNDARY_TESTS", "1")
            .args(["--sheet", "X0", "--formula", formula])
            .output()
            .unwrap();
        assert!(!output.status.success(), "{formula}");
        let error = String::from_utf8_lossy(&output.stderr);
        assert!(
            error.contains("NotModelled") && error.contains("dc-snz.18"),
            "{formula}: {error}"
        );
    }
}

/// Native-only: the x14 operand batch (dc-6332.10, answer 13 D.4 baseline and D.5) and the x15
/// foundation operands (D.0) through
/// OneCalc's executable: xlCoerce with type masks, errors and arrays, UTF-16 code units in and
/// out (embedded NUL, lone surrogate), an add-in mutating its own input array, and the module
/// name around a nested xlUDF all give Excel 16.0.20527's recorded answers (`x14-operand-cases.json`).
/// The cases reading cells other than A1 stay OneCalc's MissingInput refusals: its scratch
/// session has only that input.
#[test]
fn operand_probes_give_excel_answers() {
    let cases: Vec<serde_json::Value> = serde_json::from_str(include_str!(
        "../../../crates/calc-xll-reference/fixture/x14-operand-cases.json"
    ))
    .unwrap();
    // A Ref target returns the reference without reading it, so those two cases are served.
    let outside = |case: &serde_json::Value| {
        let formula = case["formula"].as_str().unwrap();
        ["Z99", "A1:A2", "A1:B2"]
            .iter()
            .any(|r| formula.contains(r))
            && !formula.ends_with(",8)")
    };
    let mut served = cases
        .iter()
        .filter(|c| !outside(c))
        .cloned()
        .collect::<Vec<_>>();
    assert_eq!((served.len(), cases.len()), (128, 145));
    // x15: the D.0 foundation operands as Excel builds them, decoded with the same layout.
    let foundation: Vec<serde_json::Value> = serde_json::from_str(include_str!(
        "../../../crates/calc-xll-reference/fixture/x15-foundation-cases.json"
    ))
    .unwrap();
    let before = served.len();
    // Only the two Q-typed reads of other cells refuse; U-typed ones pass the reference itself.
    served.extend(
        foundation
            .iter()
            .filter(|c| {
                !["=MyOperBytes(Z99)", "=MyOperBytes(A1:B2)"]
                    .contains(&c["formula"].as_str().unwrap())
            })
            .cloned(),
    );
    assert_eq!(served.len() - before, 19);
    let fixture = new_fixture();
    let path = fixture.directory.join("operands.json");
    std::fs::write(&path, serde_json::to_vec(&served).unwrap()).unwrap();
    let result = checked(
        fixture
            .command()
            .env("DNA_XLL_BOUNDARY_TESTS", "1")
            .args(["--sheet", "X0", "--cases"])
            .arg(&path)
            .output()
            .unwrap(),
    );
    assert_eq!(result["results"].as_array().unwrap().len(), served.len());
    assert_eq!(result["live_callback_allocations"], 0);
    let fixture = new_fixture();
    let output = fixture
        .command()
        .env("DNA_XLL_BOUNDARY_TESTS", "1")
        .args(["--sheet", "X0", "--formula", "=MyCoerceRef(Z99,1)"])
        .output()
        .unwrap();
    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("MissingInput"));
}

/// Native-only: registers ten thousand callable names in a native process, then invokes the last.
#[test]
fn ten_thousand_registrations_have_no_small_pool_limit() {
    let fixture = new_fixture();
    let result = checked(
        fixture
            .command()
            .env("DNA_XLL_EXTRA_REGISTRATIONS", "10000")
            .args(["--formula", "=Pool9999(19,23)"])
            .output()
            .unwrap(),
    );
    assert_eq!(result["registrations"], 10048);
    assert_eq!(result["results"][0]["value"], 42.);
}

/// Native-only: an XLL callback failure must survive both native swallowing and formula IFERROR.
#[test]
fn swallowed_unsupported_callbacks_remain_typed_refusals() {
    let fixture = new_fixture();
    let output = fixture
        .command()
        .args(["--formula", "=IFERROR(MyUnsupported(),123)"])
        .output()
        .unwrap();
    assert!(!output.status.success());
    let error = String::from_utf8_lossy(&output.stderr);
    assert!(
        error.contains("NotModelled") && error.contains("dc-snz.2"),
        "{error}"
    );
    assert!(output.stdout.is_empty());
}

/// Native-only: a registration can name a loaded helper DLL, whose allocator/free callback
/// must remain distinct from the XLL that issued xlfRegister.
#[test]
fn helper_module_owns_the_return_and_its_free_callback() {
    let fixture = new_fixture();
    let helper = fixture.directory.join("helper.dll");
    std::fs::copy(fixture.directory.join("fixture.xll"), &helper).unwrap();
    let result = checked(
        fixture
            .command()
            .env("DNA_XLL_HELPER", &helper)
            .args([
                "--formula",
                "=LEN(MyHelperOwned())+MyHelperFreeCount()*100+MyFreeCount()*10000",
            ])
            .output()
            .unwrap(),
    );
    assert_eq!(result["results"][0]["value"], 112.);
}

/// Native-only: failed xlAutoOpen rolls back helper-module registrations and callback storage.
#[test]
fn failed_load_rolls_back_every_registration_from_its_origin() {
    let fixture = new_fixture();
    let helper = fixture.directory.join("helper.dll");
    std::fs::copy(fixture.directory.join("fixture.xll"), &helper).unwrap();
    let output = fixture
        .command()
        .env("DNA_XLL_HELPER", &helper)
        .env("DNA_XLL_FAIL_AFTER_HELPER", "1")
        .output()
        .unwrap();
    assert!(!output.status.success());
    let error = String::from_utf8_lossy(&output.stderr);
    assert!(error.contains("xlAutoOpen returned zero"), "{error}");
    assert!(
        error.contains(
            "after rollback: 0 active registrations, 0 callable names, 0 callback allocations"
        ),
        "{error}"
    );
}

/// Native-only: a known Ftab name absent from CalcFunc must identify the XLL capability owner.
#[test]
fn known_but_unimplemented_worksheet_callbacks_keep_the_xll_owner() {
    let fixture = new_fixture();
    let output = fixture
        .command()
        .env("DNA_XLL_NEGATIVE_CALLBACK", "185")
        .args(["--formula", "=IFERROR(MyUnsupported(),123)"])
        .output()
        .unwrap();
    assert!(!output.status.success());
    let error = String::from_utf8_lossy(&output.stderr);
    assert!(
        error.contains("NotModelled") && error.contains("dc-snz.2"),
        "{error}"
    );
    assert!(!error.contains("dc-snz.5"), "{error}");
}

/// Native-only: an in-context xlStack call uses this thread's actual Windows stack bounds.
#[test]
fn stack_headroom_is_available_in_context() {
    let fixture = new_fixture();
    let output = fixture
        .command()
        .env("DNA_XLL_NEGATIVE_CALLBACK", "16385")
        .args(["--formula", "=IFERROR(MyUnsupported(),123)"])
        .output()
        .unwrap();
    let result = checked(output);
    assert_eq!(result["results"][0]["value"], 0.);
}

/// Native-only: Application.Run accepts a registered worksheet entry on the STA.
#[test]
fn application_run_can_enter_a_hidden_or_worksheet_registration() {
    let fixture = new_fixture();
    let result = checked(
        fixture
            .command()
            .env("DNA_XLL_WORKER_TESTS", "1")
            .args([
                "--formula",
                "=MyAdd(1,2)",
                "--command",
                "MyFreeCount",
                "--early-command",
                "MyFreeCount",
            ])
            .output()
            .unwrap(),
    );
    assert_eq!(result["commands"][0]["value"], 0.);
    assert_eq!(result["commands"][1]["value"], 0.);
    assert_eq!(result["commands"][1]["early_bound"], true);
}

/// Native-only: a failed second XLL must restore another load's use counts and hidden names.
#[test]
fn failed_second_load_restores_previous_names_and_use_counts() {
    let fixture = new_fixture();
    let helper = fixture.directory.join("helper.dll");
    let second = fixture.directory.join("second.xll");
    for destination in [&helper, &second] {
        std::fs::copy(fixture.directory.join("fixture.xll"), destination).unwrap();
    }
    let output = fixture
        .command()
        .args(["--xll"])
        .arg(&second)
        .env("DNA_XLL_HELPER", &helper)
        .env("DNA_XLL_FAIL_AFTER_HELPER", "1")
        .env("DNA_XLL_FAIL_MODULE", "second.xll")
        .output()
        .unwrap();
    assert!(!output.status.success());
    let error = String::from_utf8_lossy(&output.stderr);
    assert!(error.contains("xlAutoOpen returned zero"), "{error}");
    assert!(
        error.contains(
            "after rollback: 52 active registrations, 51 callable names, 0 callback allocations"
        ),
        "{error}"
    );
}

/// Native-only: unimplemented EVALUATE reads remain refusals through native swallowing and IFERROR.
#[test]
fn unimplemented_evaluate_reference_remains_a_typed_refusal() {
    let fixture = new_fixture();
    let output = fixture
        .command()
        .env("DNA_XLL_NEGATIVE_CALLBACK", "257")
        .env("DNA_XLL_EVALUATE_TEXT", "A1")
        .args(["--formula", "=IFERROR(MyUnsupported(),123)"])
        .output()
        .unwrap();
    assert!(!output.status.success());
    let error = String::from_utf8_lossy(&output.stderr);
    assert!(
        error.contains("NotModelled") && error.contains("dc-snz.2"),
        "{error}"
    );
}

/// Native-only: the product worker uses the evaluator's reference reader and function route,
/// copies DLL-owned values, and frees them on the same worker before returning to the STA.
#[test]
fn worker_calls_keep_the_main_reader_and_native_memory_ownership() {
    let fixture = new_fixture();
    let result = checked(fixture.command().env("DNA_XLL_WORKER_TESTS", "1").args([
        "--formula", "=AND(MyWorkerThread()<>MyMainThread(),MyWorkerReference(A1)=7,MyWorkerCallbacks(3,4)=7,LEN(MyWorkerOwned())=12,MyFreeCount()=1)"
    ]).output().unwrap());
    assert_eq!(result["results"][0]["value"], true);
    assert_eq!(result["live_callback_allocations"], 0);
}

/// Native-only: a worker's refusal is carried back to the same formula invocation.
#[test]
fn worker_refusals_survive_native_swallowing_and_formula_handlers() {
    let fixture = new_fixture();
    let output = fixture
        .command()
        .env("DNA_XLL_WORKER_TESTS", "1")
        .args(["--formula", "=IFERROR(MyWorkerUnsupported(),123)"])
        .output()
        .unwrap();
    assert!(!output.status.success());
    let error = String::from_utf8_lossy(&output.stderr);
    assert!(
        error.contains("NotModelled") && error.contains("dc-snz.2"),
        "{error}"
    );
}

/// Native-only: valid worker event/status requests must not succeed against a discarded snapshot.
#[test]
fn worker_state_mutations_remain_typed_refusals() {
    for callback in ["16401"] {
        let fixture = new_fixture();
        let output = fixture
            .command()
            .env("DNA_XLL_WORKER_TESTS", "1")
            .env("DNA_XLL_NEGATIVE_CALLBACK", callback)
            .args(["--formula", "=IFERROR(MyWorkerUnsupported(),123)"])
            .output()
            .unwrap();
        assert!(!output.status.success());
        let error = String::from_utf8_lossy(&output.stderr);
        assert!(
            error.contains("NotModelled") && error.contains("dc-snz.18"),
            "{callback}: {error}"
        );
    }
}

/// Native-only: a nested native call cannot silently inherit a different reference-read mode.
#[test]
fn nested_read_mode_changes_remain_typed_refusals() {
    let fixture = new_fixture();
    let output = fixture
        .command()
        .env("DNA_XLL_NEGATIVE_CALLBACK", "255")
        .args(["--formula", "=IFERROR(MyUnsupported(),123)"])
        .output()
        .unwrap();
    assert!(!output.status.success());
    let error = String::from_utf8_lossy(&output.stderr);
    assert!(
        error.contains("dc-snz.18") && error.contains("NotModelled"),
        "{error}"
    );
}

/// Native-only: constant EVALUATE expressions execute through a real native callback in OneCalc's composition.
#[test]
fn constant_evaluate_callbacks_use_the_application_formula_route() {
    for expression in ["1+1", "=SUM(3,4)*2", "SUM({1,2;3,4})"] {
        {
            let fixture = new_fixture();
            let mut command = fixture.command();
            command
                .env("DNA_XLL_NEGATIVE_CALLBACK", "257")
                .env("DNA_XLL_EVALUATE_TEXT", expression)
                .args(["--formula", "=IFERROR(MyUnsupported(),123)"]);
            let result = checked(command.output().unwrap());
            // This fixture deliberately discards the callback value. The
            // portable context regression and Excel boundary compare its value.
            assert_eq!(result["results"][0]["value"], 0.);
            assert_eq!(result["live_callback_allocations"], 0);
        }
    }
}
