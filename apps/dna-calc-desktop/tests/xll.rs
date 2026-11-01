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

/// Native-only: EVALUATE must not invent #NAME? for an expression the host cannot evaluate.
#[test]
fn unimplemented_evaluate_expression_remains_a_typed_refusal() {
    let fixture = Fixture::new();
    let output = fixture
        .command()
        .env("DNA_XLL_NEGATIVE_CALLBACK", "257")
        .env("DNA_XLL_EVALUATE_TEXT", "1+1")
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
