#![cfg(windows)]
//! DNA Calc's desktop executable (`--engine`): native add-ins through CalcHost's checked route.
#[path = "support/mod.rs"]
mod support;
use support::{Fixture, checked};

fn new_fixture() -> Fixture {
    Fixture::new(std::path::Path::new(env!("CARGO_BIN_EXE_dna-calc-desktop")))
}
/// Native-only: the actual executable resumes one native async entry through CalcHost P07.
#[test]
fn native_async_resumes_the_existing_engine_activation_and_dependents() {
    for (entry_evaluation, legacy) in [(false, false), (true, false), (true, true)] {
        let fixture = new_fixture();
        let calls = if entry_evaluation { 2 } else { 1 };
        let cases = fixture.directory.join("async-cases.json");
        std::fs::write(
        &cases,
        serde_json::to_vec(&serde_json::json!([
            {"formula":"=10+MyAsync(28)+3","value":42,"dependent":"=B3*2","dependent_value":84,"legacy":legacy,"formula_entry":entry_evaluation},
            {"formula":"=MyAsyncEntries()","value":calls,"legacy":legacy,"formula_entry":entry_evaluation}
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
        if entry_evaluation {
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

/// Native-only: two real workbook commands overlap native pool calls while callback reads stay on the owner.
#[test]
fn engine_pool_overlaps_workbooks_and_services_borrowed_reads() {
    let fixture = new_fixture();
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
    let fixture = new_fixture();
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

/// Native-only: answer 13 D.21's owner-pump witness (x16, dc-6332.10). Two thread-safe cells
/// meet at a barrier, then each reads A1 through xlCoerce, which the owner thread serves, while
/// the other is also inside; Excel 16.0.20527 gives 8 and 8 with two calls in flight
/// (`x16-pump-cases.json`), and so must the engine (a host that served one worker at a time
/// would leave both at the second barrier and answer #VALUE!).
#[test]
fn engine_owner_serves_two_pool_readers_at_once() {
    let fixture = new_fixture();
    let cases = fixture.directory.join("pump.json");
    std::fs::write(
        &cases,
        include_bytes!("../../../crates/calc-xll-reference/fixture/x16-pump-cases.json"),
    )
    .unwrap();
    let result = checked(
        fixture
            .command()
            .env("DNA_XLL_POOL_TESTS", "1")
            .args(["--engine", "--cases"])
            .arg(cases)
            .output()
            .unwrap(),
    );
    assert_eq!(result["results"][0]["value"], 8.);
    assert_eq!(result["results"][0]["dependent"], 8.);
    assert_eq!(result["results"][1]["value"], 2.);
    assert_eq!(
        result["results"][0]["provenance"]["cooperative_invocations"],
        1
    );
    assert_eq!(result["live_callback_allocations"], 0);
}

/// Native-only: answer 13 D.1/D.2's status 64 batch (x17, dc-6332.11) through the engine route.
/// A reader calculated before its producer (`Range("B3,C3").Calculate`) gets status 64 from
/// xlCoerce, its return is discarded and it is entered again after the producer; `#` reads the
/// cached value; a value argument is prepared before entry. Every case's value is Excel's.
#[test]
fn engine_status_64_matches_excel() {
    let fixture = new_fixture();
    let cases = fixture.directory.join("uncalced.json");
    let expected: Vec<serde_json::Value> = serde_json::from_slice(include_bytes!(
        "../../../crates/calc-xll-reference/fixture/x17-uncalced-cases.json"
    ))
    .unwrap();
    std::fs::write(&cases, serde_json::to_vec(&expected).unwrap()).unwrap();
    let result = checked(
        fixture
            .command()
            .env("DNA_XLL_UNCALCED_TESTS", "1")
            .env("DNA_XLL_POOL_TESTS", "1")
            .args(["--engine", "--cases"])
            .arg(cases)
            .output()
            .unwrap(),
    );
    let results = result["results"].as_array().unwrap();
    assert_eq!(results.len(), expected.len());
    for (row, case) in results.iter().zip(&expected) {
        match case["value"].as_f64() {
            Some(number) => assert_eq!(row["value"].as_f64(), Some(number), "{}", case["formula"]),
            None => assert_eq!(row["value"], case["value"], "{}", case["formula"]),
        }
    }
    assert_eq!(result["live_callback_allocations"], 0);
}

/// Native-only: overlapping native array producers use the real engine and DLL-owned frees.
#[test]
fn engine_pool_overlaps_spill_producers_and_publishes_every_cell() {
    let fixture = new_fixture();
    let cases = fixture.directory.join("spill-pool.json");
    std::fs::write(
        &cases,
        include_bytes!("../../../crates/calc-xll-reference/fixture/x8-spill-pool-cases.json"),
    )
    .unwrap();
    let output = checked(
        fixture
            .command()
            .env("DNA_XLL_POOL_TESTS", "1")
            .env("DNA_XLL_WORKER_TESTS", "1")
            .args(["--engine", "--cases"])
            .arg(cases)
            .output()
            .unwrap(),
    );
    assert_eq!(output["results"][1]["value"], 2.);
    assert_eq!(output["results"][2]["value"], 3.);
    let cells = output["results"][0]["spill_cells"].as_object().unwrap();
    assert_eq!(cells.len(), 8);
    for (address, expected) in [
        ("B3", 41.),
        ("C3", 42.),
        ("B4", 43.),
        ("C4", 44.),
        ("F3", 81.),
        ("G3", 82.),
        ("F4", 83.),
        ("G4", 84.),
    ] {
        assert_eq!(cells[address]["value"], expected);
        assert_eq!(cells[address]["provenance"]["completed_invocations"], 1);
        assert_eq!(cells[address]["provenance"]["cooperative_invocations"], 1);
    }
}

/// Native-only: overwrite revokes the engine token and the delayed XLL return cannot publish or unload early.
#[test]
fn overwritten_native_async_call_rejects_its_late_return() {
    let fixture = new_fixture();
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

/// Native-only: CalcHost's teardown enters only xlAutoClose, as Excel's UNREGISTER does (X2 traces).
#[test]
fn engine_teardown_enters_only_xl_auto_close() {
    let fixture = new_fixture();
    let log = fixture.directory.join("lifecycle.log");
    let result = checked(
        fixture
            .command()
            .env("DNA_XLL_LIFECYCLE_LOG", &log)
            .args(["--engine", "--formula", "=MyAdd(1,2)"])
            .output()
            .unwrap(),
    );
    assert_eq!(result["results"][0]["value"], 3.);
    assert_eq!(
        std::fs::read_to_string(&log).unwrap(),
        "xlAutoOpen\nxlAutoClose\n"
    );
}

/// Native-only: preparation failures in cooperative native routes remain counted refusals through IFERROR.
#[test]
fn cooperative_native_abi_refusals_cannot_be_swallowed() {
    for formula in [
        "=IFERROR(MyPoolUnsupportedAbi(),123)",
        "=IFERROR(MyAsyncUnsupportedAbi(1),123)",
    ] {
        let fixture = new_fixture();
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

/// Native-only: measured denied calls return Excel's code without changing the owner, while # runs its allowed mutation.
#[test]
fn measured_worker_denial_and_macro_message_permissions() {
    let fixture = new_fixture();
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
    let fixture = new_fixture();
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

/// Native-only: constant EVALUATE expressions execute through a real native callback in DNA Calc's composition.
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
            command.arg("--engine");
            let result = checked(command.output().unwrap());
            // This fixture deliberately discards the callback value. The
            // portable context regression and Excel boundary compare its value.
            assert_eq!(result["results"][0]["value"], 0.);
            assert_eq!(result["live_callback_allocations"], 0);
        }
    }
}

/// Native-only: `Excel12(xlfCall)` and `Excel12(xlfRegisterId)` run in the engine route with the
/// answers Excel 16.0.20527 gave to the same fixture formulas (dc-gh22). The full measured set
/// is `crates/calc-xll-reference/fixture/x1-cases.json`; this keeps one case per rule.
#[test]
fn call_and_register_id_match_the_measured_excel_answers() {
    let fixture = new_fixture();
    let cases = fixture.directory.join("call-cases.json");
    let expected = [
        (
            r#"=MyXlf(150,"kernel32","lstrlenA","JC","hello")"#,
            "0|num|5",
        ),
        (r#"=MyXlf(150,"@self","MyAdd","BBB",1,2)"#, "0|num|3"),
        (
            r#"=MyXlf(150,"@self","MyAdd","BBB",1,2,3)"#,
            "0|err|#VALUE!",
        ),
        (
            r#"=MyXlf(150,"@self","MyShortAdd","III",40000,1)"#,
            "0|err|#NUM!",
        ),
        (
            r#"=MyXlf(150,MyXlfRaw(267,"@self","MyAdd"),1,2)"#,
            "0|num|3",
        ),
        (
            r#"=MyXlf(150,"@self","NoSuchProc","BBB",1,2)"#,
            "0|err|#VALUE!",
        ),
        (r#"=MyXlf(150)"#, "4|err|#VALUE!"),
        (
            r#"=MyXlfPlain(150,"@self","MyAdd","BBB",1,2)"#,
            "2|err|#VALUE!",
        ),
        (
            r#"=MyRegIdCheck("MyAdd","MyAdd")"#,
            "0|num|same=true|integer=true",
        ),
        (
            r#"=MyRegIdRepeat("kernel32","lstrlenA","JC")"#,
            "codes=0,0,0|first_repeat_same=true|case_same=true",
        ),
    ];
    std::fs::write(
        &cases,
        serde_json::to_vec(
            &expected
                .iter()
                .map(|(formula, value)| serde_json::json!({"formula": formula, "value": value}))
                .collect::<Vec<_>>(),
        )
        .unwrap(),
    )
    .unwrap();
    let result = checked(
        fixture
            .command()
            .arg("--engine")
            .arg("--cases")
            .arg(&cases)
            .output()
            .unwrap(),
    );
    let results = result["results"].as_array().unwrap();
    assert_eq!(results.len(), expected.len());
    for (row, (formula, value)) in results.iter().zip(expected) {
        assert_eq!(row["value"], value, "{formula}");
    }
    assert_eq!(result["live_callback_allocations"], 0);
}

/// The VBA add-in and macro workbook of the simple-UDF set (crates/calc-xll-reference/fixture/vba),
/// copied beside the executable under test.
fn vba_fixture(fixture: &Fixture, name: &str) -> std::path::PathBuf {
    let source = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../crates/calc-xll-reference/fixture/vba")
        .join(name);
    let target = fixture.directory.join(name);
    std::fs::copy(source, &target).unwrap();
    target
}

/// Native-only: the executable loads a VBA .xlam through --xll (the option app mode takes too);
/// Excel's answers are those of simple-udf-cases.json and x9-vba-cases.json.
#[test]
fn an_xlam_loads_through_the_addin_option_and_answers_as_excel() {
    let fixture = new_fixture();
    let xlam = vba_fixture(&fixture, "DnaExamples.xlam");
    let cases = fixture.directory.join("vba-cases.json");
    let expected = serde_json::json!([
        {"formula":"=DnaAdd(40,2)","value":42.0},
        {"formula":"=SimpleDims({1,2,3})","value":"Variant():1-3:1d"},
        {"formula":"=SimpleDims({1;2;3})","value":"Variant():1-3x1-1"},
        {"formula":"=SimpleAsLong(3000000000)","value":{"error":"#NUM!"}},
        {"formula":"=SimpleOverflow(100000)","value":{"error":"#VALUE!"}},
        {"formula":"=SimpleDiv(1,0)","value":{"error":"#DIV/0!"}},
        {"formula":"=INDEX(SimpleGrid(),2,3)","value":23.0},
        {"formula":"=DnaOpenCount()","value":1.0}
    ]);
    std::fs::write(&cases, serde_json::to_vec(&expected).unwrap()).unwrap();
    let result = checked(
        fixture
            .command()
            .arg("--xll")
            .arg(&xlam)
            .arg("--engine")
            .arg("--cases")
            .arg(&cases)
            .output()
            .unwrap(),
    );
    for (case, result) in expected.as_array().unwrap().iter().zip(result["results"].as_array().unwrap()) {
        assert_eq!(result["value"], case["value"], "{}", case["formula"]);
    }
}

/// Native-only: a macro workbook's own VBA functions answer beside a native add-in's, through the
/// open-time route the page's worker takes (calc-host-vba), once the person enables macros
/// (`--enable-macros`, dc-0f1k.22). `local.xlsm`'s Module1.MyFunc is x+100
/// (docs/plan/VBA_UDF_NAMES_IN_FILES.md); MyAdd is the X1 fixture's.
#[test]
fn a_macro_workbook_answers_its_own_functions_beside_a_native_addin() {
    let fixture = new_fixture();
    let xlsm = fixture.directory.join("local.xlsm");
    std::fs::copy(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../crates/calc-doc/tests/fixtures/vba_udf_names/local.xlsm"),
        &xlsm,
    )
    .unwrap();
    let result = checked(
        fixture
            .command()
            .arg("--engine")
            .arg("--workbook")
            .arg(&xlsm)
            .arg("--enable-macros")
            .args(["--formula", "=MyFunc(2)+MyAdd(1,2)"])
            .output()
            .unwrap(),
    );
    assert_eq!(result["workbook_project"], true);
    assert_eq!(result["results"][0]["value"], 105.0);
}

/// Native-only: without `--enable-macros` the workbook's project is held, not run, so its
/// function does not answer; with the Mark of the Web (zone 3) enabling is refused (dc-0f1k.22).
#[test]
fn a_macro_workbook_runs_nothing_until_enabled_and_a_marked_one_never() {
    let fixture = new_fixture();
    let xlsm = fixture.directory.join("held.xlsm");
    std::fs::copy(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../crates/calc-doc/tests/fixtures/vba_udf_names/local.xlsm"),
        &xlsm,
    )
    .unwrap();
    let held = checked(
        fixture
            .command()
            .arg("--engine")
            .arg("--workbook")
            .arg(&xlsm)
            .args(["--formula", "=MyFunc(2)"])
            .output()
            .unwrap(),
    );
    assert_eq!(held["workbook_project"], false);
    assert_ne!(held["results"][0]["value"], 102.0, "{held}");
    let mut stream = xlsm.as_os_str().to_owned();
    stream.push(":Zone.Identifier");
    std::fs::write(std::path::PathBuf::from(stream), "[ZoneTransfer]\r\nZoneId=3\r\n").unwrap();
    let blocked = checked(
        fixture
            .command()
            .arg("--engine")
            .arg("--workbook")
            .arg(&xlsm)
            .arg("--enable-macros")
            .args(["--formula", "=MyFunc(2)"])
            .output()
            .unwrap(),
    );
    assert_eq!(blocked["workbook_project"], false);
    assert!(
        blocked["macros_refused"]
            .as_str()
            .is_some_and(|why| why.contains("Mark of the Web")),
        "{blocked}"
    );
    assert_ne!(blocked["results"][0]["value"], 102.0, "{blocked}");
}

/// Native-only: the macro security level in the harness (dc-15dk), through the real executable and
/// its per-user settings folder. A marked file is refused by `--enable-macros` and runs under
/// `--trust-macros`, `--macros unguarded`, `DNACALC_TRUST_ALL_MACROS=1` and the stored level
/// Unguarded and Safe (the project runs under CalcVBA's Safe policy); the flag and the
/// variable pin Unguarded over a stored level while `--macros off` pins Off over them all.
/// `local.xlsm`'s Module1.MyFunc is x+100.
#[test]
fn the_macro_security_level_decides_whether_a_marked_workbook_runs_in_the_harness() {
    let fixture = new_fixture();
    let settings = fixture.directory.join("settings");
    let xlsm = fixture.directory.join("marked.xlsm");
    std::fs::copy(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../crates/calc-doc/tests/fixtures/vba_udf_names/local.xlsm"),
        &xlsm,
    )
    .unwrap();
    let mut stream = xlsm.as_os_str().to_owned();
    stream.push(":Zone.Identifier");
    std::fs::write(std::path::PathBuf::from(stream), "[ZoneTransfer]\r\nZoneId=3\r\n").unwrap();
    let run =|args: &[&str], variable: Option<&str>| {
        let mut command = fixture.command();
        command
            .env("DNACALC_SETTINGS_DIR", &settings)
            .env_remove("DNACALC_TRUST_ALL_MACROS");
        if let Some(value) = variable {
            command.env("DNACALC_TRUST_ALL_MACROS", value);
        }
        checked(
            command
                .arg("--engine")
                .arg("--workbook")
                .arg(&xlsm)
                .args(args)
                .args(["--formula", "=MyFunc(2)"])
                .output()
                .unwrap(),
        )
    };
    let runs = |result: &serde_json::Value| {
        result["workbook_project"] == true && result["results"][0]["value"] == 102.0
    };
    // Off (nothing stored): the plain enable is refused for the marked file.
    let off = run(&["--enable-macros"], None);
    assert!(!runs(&off) && off["macros_refused"].is_string(), "{off}");
    // The flags and the variable run it (and need no `--enable-macros`).
    assert!(runs(&run(&["--trust-macros"], None)));
    assert!(runs(&run(&["--macros", "unguarded"], None)));
    assert!(runs(&run(&[], Some("1"))));
    assert!(!runs(&run(&[], Some("0"))));
    assert!(!runs(&run(&["--macros", "off"], Some("1"))));
    // The stored levels.
    let store = |level: &str| {
        std::fs::create_dir_all(&settings).unwrap();
        std::fs::write(settings.join("macro-security.txt"), level).unwrap();
    };
    store("unguarded");
    assert!(runs(&run(&[], None)));
    store("safe");
    assert!(runs(&run(&[], None)), "Safe runs the project under the Safe policy");
    // The flag and the variable pin Unguarded over the stored Safe.
    assert!(runs(&run(&["--trust-macros"], None)));
    assert!(runs(&run(&[], Some("1"))));
}
