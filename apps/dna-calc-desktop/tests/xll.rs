#![cfg(windows)]
use std::{
    path::{Path, PathBuf},
    process::{Command, Output},
    sync::atomic::{AtomicUsize, Ordering},
};
static NEXT: AtomicUsize = AtomicUsize::new(0);
struct Fixture {
    directory: PathBuf,
}
impl Fixture {
    fn new() -> Self {
        assert_eq!(calc_xll_fixture::abi_version(), 12);
        assert_eq!(xlcall32::XLCallVer(), 3072);
        let directory = std::env::temp_dir().join(format!(
            "dnacalc-xll-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir(&directory).unwrap();
        let executable = Path::new(env!("CARGO_BIN_EXE_dna-calc-desktop"));
        std::fs::copy(executable, directory.join("dna-calc-desktop.exe")).unwrap();
        for (source, target) in [
            ("calc_xll_fixture.dll", "fixture.xll"),
            ("xlcall32.dll", "XLCALL32.DLL"),
        ] {
            let root = executable.parent().unwrap();
            let candidates = [root.join("deps").join(source), root.join(source)];
            let source = candidates
                .iter()
                .find(|p| p.is_file())
                .unwrap_or_else(|| panic!("missing built {source}"));
            std::fs::copy(source, directory.join(target)).unwrap();
        }
        Self { directory }
    }
    fn command(&self) -> Command {
        let mut command = Command::new(self.directory.join("dna-calc-desktop.exe"));
        command
            .args(["--xll"])
            .arg(self.directory.join("fixture.xll"));
        command
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        let resolved = std::fs::canonicalize(&self.directory).unwrap();
        let root = std::fs::canonicalize(std::env::temp_dir()).unwrap();
        assert_eq!(resolved.parent(), Some(root.as_path()));
        assert!(
            resolved
                .file_name()
                .unwrap()
                .to_string_lossy()
                .starts_with(&format!("dnacalc-xll-{}-", std::process::id()))
        );
        std::fs::remove_dir_all(resolved).unwrap();
    }
}
fn checked(output: Output) -> serde_json::Value {
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    serde_json::from_slice(&output.stdout).unwrap()
}
/// Native-only: loads a native XLL in the exported Windows executable and calculates formulas.
#[test]
fn real_executable_uses_onecalc_for_the_same_excel_fixture() {
    let fixture = Fixture::new();
    let result = checked(fixture.command().arg("--self-test").output().unwrap());
    assert_eq!(result["results"].as_array().unwrap().len(), 21);
    assert_eq!(result["results"][0]["value"], 3.);
    assert_eq!(result["results"][16]["value"], 41.);
    assert_eq!(result["results"][17]["value"], 42.);
    assert_eq!(result["duplicate_use_count"], 2);
    assert_eq!(result["live_callback_allocations"], 0);
    assert_eq!(result["closed_registration"], "#NAME?");
}
/// Native-only: invokes typed ABI entries through the application and real XLL.
#[test]
fn typed_native_arguments_coerce_values_and_return_worksheet_errors() {
    let fixture = Fixture::new();
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
    let fixture = Fixture::new();
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
    let fixture = Fixture::new();
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
/// Native-only: registers ten thousand callable names in a native process, then invokes the last.
#[test]
fn ten_thousand_registrations_have_no_small_pool_limit() {
    let fixture = Fixture::new();
    let result = checked(
        fixture
            .command()
            .env("DNA_XLL_EXTRA_REGISTRATIONS", "10000")
            .args(["--formula", "=Pool9999(19,23)"])
            .output()
            .unwrap(),
    );
    assert_eq!(result["registrations"], 10018);
    assert_eq!(result["results"][0]["value"], 42.);
}
/// Native-only: an XLL callback failure must survive both native swallowing and formula IFERROR.
#[test]
fn swallowed_unsupported_callbacks_remain_typed_refusals() {
    let fixture = Fixture::new();
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
    let fixture = Fixture::new();
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
    let fixture = Fixture::new();
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
    let fixture = Fixture::new();
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
    let fixture = Fixture::new();
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
    let fixture = Fixture::new();
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

/// Native-only: the actual executable resumes one native async entry through CalcHost P07.
#[test]
fn native_async_resumes_the_existing_engine_activation_and_dependents() {
    for legacy in [false, true] {
        let fixture = Fixture::new();
        let calls = if legacy { 2 } else { 1 };
        let cases = fixture.directory.join("async-cases.json");
        std::fs::write(
        &cases,
        serde_json::to_vec(&serde_json::json!([
            {"formula":"=10+MyAsync(28)+3","value":42,"dependent":"=B3*2","dependent_value":84,"legacy":legacy},
            {"formula":"=MyAsyncEntries()","value":calls,"legacy":legacy}
        ])).unwrap(),
    )
    .unwrap();
        let result = checked(
            fixture
                .command()
                .env("DNA_XLL_ASYNC_TESTS", "1")
                .env("DNA_XLL_ASYNC_DELAY_MS", "30")
                .arg("--engine")
                .arg("--cases")
                .arg(&cases)
                .output()
                .unwrap(),
        );
        assert_eq!(result["async_returns"].as_array().unwrap().len(), calls);
        assert_eq!(result["async_returns"][0]["disposition"], "Accepted");
        assert_eq!(result["results"][0]["value"], 42.);
        assert_eq!(result["results"][0]["dependent"], 84.);
        assert_eq!(result["results"][0]["provenance"]["invocations"], 1);
        assert_eq!(
            result["results"][0]["provenance"]["cooperative_invocations"],
            1
        );
        assert_eq!(
            result["results"][0]["provenance"]["completed_invocations"],
            1
        );
        assert_eq!(result["results"][1]["value"].as_f64(), Some(calls as f64));
        if legacy {
            let entries = result["formula_entries"].as_array().unwrap();
            let entry = entries
                .iter()
                .find(|row| row["cell"] == "B3" && row["formula"] == "=10+MyAsync(28)+3")
                .unwrap();
            assert_eq!(entry["value"], 42.);
            assert!(
                entry["provenance"]["stamp"]
                    .as_str()
                    .unwrap()
                    .contains("reason: FormulaEntry")
            );
            assert_eq!(entry["provenance"]["cooperative_invocations"], 1);
            assert_eq!(entry["provenance"]["completed_invocations"], 1);
        }
        assert_eq!(result["closed_registration"], "#NAME?");
    }
}

/// Native-only: a failed second XLL must restore another load's use counts and hidden names.
#[test]
fn failed_second_load_restores_previous_names_and_use_counts() {
    let fixture = Fixture::new();
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
            "after rollback: 22 active registrations, 22 callable names, 0 callback allocations"
        ),
        "{error}"
    );
}

/// Native-only: unimplemented EVALUATE reads remain refusals through native swallowing and IFERROR.
#[test]
fn unimplemented_evaluate_reference_remains_a_typed_refusal() {
    let fixture = Fixture::new();
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

/// Native-only: constant EVALUATE expressions execute through a real native callback in both compositions.
#[test]
fn constant_evaluate_callbacks_use_the_application_formula_route() {
    for expression in ["1+1", "=SUM(3,4)*2", "SUM({1,2;3,4})"] {
        for engine in [false, true] {
            let fixture = Fixture::new();
            let mut command = fixture.command();
            command
                .env("DNA_XLL_NEGATIVE_CALLBACK", "257")
                .env("DNA_XLL_EVALUATE_TEXT", expression)
                .args(["--formula", "=IFERROR(MyUnsupported(),123)"]);
            if engine {
                command.arg("--engine");
            }
            let result = checked(command.output().unwrap());
            // This fixture deliberately discards the callback value. The
            // portable context regression and Excel boundary compare its value.
            assert_eq!(result["results"][0]["value"], 0.);
            assert_eq!(result["live_callback_allocations"], 0);
        }
    }
}

/// Native-only: the product worker uses the evaluator's reference reader and function route,
/// copies DLL-owned values, and frees them on the same worker before returning to the STA.
#[test]
fn worker_calls_keep_the_main_reader_and_native_memory_ownership() {
    let fixture = Fixture::new();
    let result = checked(fixture.command().env("DNA_XLL_WORKER_TESTS", "1").args([
        "--formula", "=AND(MyWorkerThread()<>MyMainThread(),MyWorkerReference(A1)=7,MyWorkerCallbacks(3,4)=7,LEN(MyWorkerOwned())=12,MyFreeCount()=1)"
    ]).output().unwrap());
    assert_eq!(result["results"][0]["value"], true);
    assert_eq!(result["live_callback_allocations"], 0);
}

/// Native-only: a worker's refusal is carried back to the same formula invocation.
#[test]
fn worker_refusals_survive_native_swallowing_and_formula_handlers() {
    let fixture = Fixture::new();
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

/// Native-only: two real workbook commands overlap native pool calls while callback reads stay on the owner.
#[test]
fn engine_pool_overlaps_workbooks_and_services_borrowed_reads() {
    let fixture = Fixture::new();
    let cases = fixture.directory.join("engine-pool.json");
    std::fs::write(&cases, serde_json::to_vec(&serde_json::json!([
        {"formula":"=MyOverlap(40)","value":41,"concurrent_formula":"=MyOverlap(80)","concurrent_value":81},
        {"formula":"=MyOverlapMax()","value":2},
        {"formula":"=MyWorkerCallbacks(19,23)","value":42},
        {"formula":"=MyWorkerReference(A1)","value":7},
        {"formula":"=MyWorkerOwned()","value":"native owned"},
        {"formula":"=MyFreeCount()","value":1}
    ])).unwrap()).unwrap();
    let result = checked(
        fixture
            .command()
            .env("DNA_XLL_POOL_TESTS", "1")
            .env("DNA_XLL_WORKER_TESTS", "1")
            .args(["--engine", "--cases"])
            .arg(cases)
            .output()
            .unwrap(),
    );
    assert_eq!(result["results"][0]["value"], 41.);
    assert_eq!(result["results"][0]["concurrent"]["value"], 81.);
    for proof in [
        &result["results"][0]["provenance"],
        &result["results"][0]["concurrent"]["provenance"],
        &result["results"][2]["provenance"],
        &result["results"][3]["provenance"],
    ] {
        assert_eq!(proof["cooperative_invocations"], 1);
        assert_eq!(proof["completed_invocations"], 1);
    }
    assert_eq!(result["results"][1]["value"], 2.);
    assert_eq!(result["live_callback_allocations"], 0);
}

/// Native-only: real XLL barriers and worker callbacks run through one workbook command.
#[test]
fn engine_pool_overlaps_independent_cells_in_one_workbook() {
    let fixture = Fixture::new();
    let cases = fixture.directory.join("workbook-pool.json");
    std::fs::write(
        &cases,
        include_bytes!("../../../crates/calc-xll-reference/fixture/x5-workbook-pool-cases.json"),
    )
    .unwrap();
    let result = checked(
        fixture
            .command()
            .env("DNA_XLL_POOL_TESTS", "1")
            .env("DNA_XLL_WORKER_TESTS", "1")
            .args(["--engine", "--cases"])
            .arg(cases)
            .output()
            .unwrap(),
    );
    assert_eq!(result["results"][0]["value"], 41.);
    assert_eq!(result["results"][0]["dependent"], 81.);
    assert_eq!(result["results"][1]["value"], 2.);
    assert_eq!(result["results"][5]["value"], 2.);
    let entries = result["formula_entries"].as_array().unwrap();
    assert_eq!(entries.len(), 8);
    for row in entries {
        assert!(
            row["provenance"]["stamp"]
                .as_str()
                .unwrap()
                .contains("reason: FormulaEntry")
        );
        assert_eq!(row["provenance"]["cooperative_invocations"], 0);
    }
    for proof in [
        &result["results"][0]["provenance"],
        &result["results"][0]["dependent_provenance"],
        &result["results"][2]["provenance"],
        &result["results"][3]["provenance"],
    ] {
        assert_eq!(proof["cooperative_invocations"], 1);
        assert_eq!(proof["completed_invocations"], 1);
    }
    assert_eq!(result["live_callback_allocations"], 0);
}

/// Native-only: overwrite revokes the engine token and the delayed XLL return cannot publish or unload early.
#[test]
fn overwritten_native_async_call_rejects_its_late_return() {
    let fixture = Fixture::new();
    let cases = fixture.directory.join("engine-overwrite.json");
    std::fs::write(&cases, serde_json::to_vec(&serde_json::json!([
        {"formula":"=10+MyAsync(28)+3","replace_pending":"=99","value":99,"dependent":"=B3*2","dependent_value":198},
        {"formula":"=MyAsyncEntries()","value":1},
        {"formula":"=MyAsyncReturns()","value":1},
        {"formula":"=MyAsyncLastCode()","value":256}
    ])).unwrap()).unwrap();
    let result = checked(
        fixture
            .command()
            .env("DNA_XLL_ASYNC_TESTS", "1")
            .env("DNA_XLL_ASYNC_DELAY_MS", "300")
            .args(["--engine", "--cases"])
            .arg(cases)
            .output()
            .unwrap(),
    );
    assert_eq!(result["revoked_tokens"].as_array().unwrap().len(), 1);
    assert_eq!(
        result["revoked_tokens"][0]["disposition"],
        "Rejected(Revoked)"
    );
    assert!(result["async_returns"].as_array().unwrap().is_empty());
    assert_eq!(result["results"][0]["value"], 99.);
    assert_eq!(result["results"][0]["dependent"], 198.);
    assert_eq!(result["results"][3]["value"], 256.);
}

/// Native-only: preparation failures in cooperative native routes remain counted refusals through IFERROR.
#[test]
fn cooperative_native_abi_refusals_cannot_be_swallowed() {
    for formula in [
        "=IFERROR(MyPoolUnsupportedAbi(),123)",
        "=IFERROR(MyAsyncUnsupportedAbi(1),123)",
    ] {
        let fixture = Fixture::new();
        let result = fixture
            .command()
            .env("DNA_XLL_ABI_NEGATIVE_TESTS", "1")
            .args(["--engine", "--formula", formula])
            .output()
            .unwrap();
        assert!(!result.status.success());
        let error = String::from_utf8_lossy(&result.stderr);
        assert!(
            error.contains("NotModelled") && error.contains("dc-snz.2"),
            "{formula}: {error}"
        );
    }
}

/// Native-only: valid worker event/status requests must not succeed against a discarded snapshot.
#[test]
fn worker_state_mutations_remain_typed_refusals() {
    for callback in ["16401"] {
        let fixture = Fixture::new();
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

/// Native-only: measured denied calls return Excel's code without changing the owner, while # runs its allowed mutation.
#[test]
fn measured_worker_denial_and_macro_message_permissions() {
    let fixture = Fixture::new();
    let result = checked(
        fixture
            .command()
            .env("DNA_XLL_WORKER_TESTS", "1")
            .args([
                "--engine",
                "--formula",
                "=IFERROR(MyWorkerDeniedStatus(),123)",
            ])
            .output()
            .unwrap(),
    );
    assert_eq!(result["results"][0]["value"], 128.);
    assert!(result["status_message"].is_null());
    let fixture = Fixture::new();
    let result = checked(
        fixture
            .command()
            .env("DNA_XLL_WORKER_TESTS", "1")
            .env("DNA_XLL_NEGATIVE_CALLBACK", "32890")
            .args(["--engine", "--formula", "=MyMacroSetStatus()"])
            .output()
            .unwrap(),
    );
    assert_eq!(result["results"][0]["value"], 0.);
    assert_eq!(result["status_message"], "worker status");
}

/// Native-only: a nested native call cannot silently inherit a different reference-read mode.
#[test]
fn nested_read_mode_changes_remain_typed_refusals() {
    let fixture = Fixture::new();
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
