//! Unit tests of `cargo xtask corpus` (`corpus.rs`): its arguments, the workbooks it finds, and
//! the capture cache's keys, lookups and imports. None of them starts Excel or a compare process.

use super::*;

fn args(text: &str) -> Vec<String> {
    text.split_whitespace().map(str::to_owned).collect()
}

/// A fresh, empty folder under the system's temporary folder.
fn scratch(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("xtask-corpus-{}-{name}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    dir
}

const SHA: &str = "1f75eb46e93e54b486542025de142abd2d036255d8c9c1effa34b4a624ff06e5";

/// A version 2 capture record of the workbook `SHA` taken by Excel `build`, one formula cell.
fn record(build: &str, case: &str) -> String {
    format!(
        r#"{{"format": "dnacalc-capture-record", "version": 2, "case": "{case}", "source": "sha256:{SHA}",
         "acquisition": {{"kind": "live_capture"}},
         "environment": {{"excel_build": "{build}", "channel": "current", "compatibility": "calc-1",
           "numeric_environment": "excel-x86-64/cpu-x86-64", "settings": {{"date1904": "false"}}}},
         "session": {{"revision": 0, "operations": [], "context": "c", "tool": "t",
           "packages": {{"input": "sha256:{SHA}", "output": "sha256:out"}}, "failures": []}},
         "sheets": [{{"name": "Sheet1", "visibility": "visible"}}],
         "formulas": [{{"sheet": "Sheet1", "row": 1, "column": 1, "kind": "ordinary", "formula": "=1+1"}}],
         "shared_ranges": [],
         "observations": [{{"sheet": "Sheet1", "row": 1, "column": 1, "excel_raw": {{"number": "4000000000000000"}}}}],
         "saved": [], "reopened": []}}"#
    )
}

#[test]
fn arguments_have_their_defaults_and_refusals() {
    let parsed = parse_args(&args("corpus-dir")).unwrap();
    assert_eq!(
        parsed,
        Args {
            dir: Some(PathBuf::from("corpus-dir")),
            oracle: false,
            out: PathBuf::from(DEFAULT_OUT),
            cache: PathBuf::from(DEFAULT_CACHE),
            imports: Vec::new(),
            no_capture: false,
            timeout: Duration::from_secs(DEFAULT_TIMEOUT_S),
        }
    );
    let parsed = parse_args(&args(
        "--oracle --out o --cache c --import a.json --import b.json --no-capture --timeout 60",
    ))
    .unwrap();
    assert_eq!(parsed.dir, None);
    assert!(parsed.oracle && parsed.no_capture);
    assert_eq!(parsed.out, PathBuf::from("o"));
    assert_eq!(parsed.cache, PathBuf::from("c"));
    assert_eq!(
        parsed.imports,
        [PathBuf::from("a.json"), PathBuf::from("b.json")]
    );
    assert_eq!(parsed.timeout, Duration::from_secs(60));
    for (bad, refusal) in [
        ("", "give a workbook folder, --oracle or --import"),
        ("--out o", "give a workbook folder"),
        ("a b", "only one workbook folder"),
        ("a --out", "--out needs a value"),
        ("a --out o --out p", "--out is given twice"),
        ("a --timeout 0", "not a positive number"),
        ("a --capture x", "unknown argument"),
    ] {
        let error = parse_args(&args(bad)).unwrap_err();
        assert!(error.contains(refusal), "{bad:?}: {error}");
    }
}

#[test]
fn file_safe_names_keep_letters_digits_dots_and_dashes() {
    assert_eq!(
        file_safe("calc-func/dc_nzpw/oracle.xlsx"),
        "calc-func_dc_nzpw_oracle.xlsx"
    );
    assert_eq!(file_safe("16.0.20430.20182"), "16.0.20430.20182");
    assert_eq!(file_safe("a b:c"), "a_b_c");
}

/// The workbooks of a folder: xlsx and xlsm files directly in it, in name order, without Excel's
/// lock files, other files or subfolders.
#[test]
fn a_folder_lists_its_workbooks_in_name_order() {
    let dir = scratch("folder");
    for name in ["b.xlsx", "a.XLSM", "~$b.xlsx", "notes.txt", "c.xls"] {
        fs::write(dir.join(name), b"x").unwrap();
    }
    fs::create_dir_all(dir.join("sub")).unwrap();
    fs::write(dir.join("sub").join("d.xlsx"), b"x").unwrap();
    let names: Vec<String> = folder_workbooks(&dir)
        .unwrap()
        .into_iter()
        .map(|entry| entry.name)
        .collect();
    assert_eq!(names, ["a.XLSM", "b.xlsx"]);
    assert!(folder_workbooks(&dir.join("absent")).is_err());
    let _ = fs::remove_dir_all(&dir);
}

/// The oracle workbooks of a repository tree, each with its committed capture when there is one.
#[test]
fn oracle_workbooks_pair_with_their_committed_captures() {
    let root = scratch("oracle");
    let fixture = |krate: &str, case: &str| {
        let dir = root
            .join("crates")
            .join(krate)
            .join("tests")
            .join("fixtures")
            .join(case);
        fs::create_dir_all(&dir).unwrap();
        fs::write(dir.join("oracle.xlsx"), b"x").unwrap();
    };
    fixture("calc-func", "f1");
    fixture("calc-func", "f2");
    fixture("calc-fml", "l1");
    let captured = root
        .join("crates")
        .join("calc-func")
        .join("tests")
        .join("captures")
        .join("f1");
    fs::create_dir_all(&captured).unwrap();
    fs::write(captured.join("oracle.capture.json"), b"{}").unwrap();
    // A fixture folder without an oracle workbook is not one.
    fs::create_dir_all(
        root.join("crates")
            .join("calc-fml")
            .join("tests")
            .join("fixtures")
            .join("other"),
    )
    .unwrap();
    let entries = oracle_workbooks(&root).unwrap();
    let listed: Vec<(String, bool)> = entries
        .iter()
        .map(|entry| (entry.name.clone(), entry.committed.is_some()))
        .collect();
    assert_eq!(
        listed,
        [
            ("calc-fml/l1/oracle.xlsx".to_owned(), false),
            ("calc-func/f1/oracle.xlsx".to_owned(), true),
            ("calc-func/f2/oracle.xlsx".to_owned(), false),
        ]
    );
    let _ = fs::remove_dir_all(&root);
}

/// The cache keys a record by workbook hash and Excel build, finds it for that build only (or,
/// when the build is not known, for any), never overwrites a cached record, and imports a record
/// by the keys it names.
#[test]
fn the_cache_is_keyed_by_workbook_hash_and_excel_build() {
    let dir = scratch("cache");
    let cache = Cache {
        root: dir.join("cache"),
    };
    assert_eq!(
        cache.path(SHA, "16.0.1"),
        dir.join("cache").join(SHA).join("16.0.1.capture.json")
    );
    assert_eq!(cache.find(SHA, Some("16.0.1")), None);
    assert_eq!(cache.find(SHA, None), None);

    let source = dir.join("a.capture.json");
    fs::write(&source, record("16.0.1", "a")).unwrap();
    let message = cache.import(&source).unwrap();
    assert!(message.starts_with("imported"), "{message}");
    assert_eq!(
        cache.find(SHA, Some("16.0.1")),
        Some((cache.path(SHA, "16.0.1"), "16.0.1".to_owned()))
    );
    assert_eq!(cache.find(SHA, Some("16.0.2")), None);
    assert_eq!(
        cache.find(SHA, None).map(|(_, build)| build),
        Some("16.0.1".to_owned())
    );
    // The same record again is cached already; another record of the same keys is not imported
    // and the cached one is kept.
    let again = cache.import(&source).unwrap();
    assert!(again.contains("is cached already"), "{again}");
    let other = dir.join("b.capture.json");
    fs::write(&other, record("16.0.1", "b")).unwrap();
    let kept = cache.import(&other).unwrap();
    assert!(kept.contains("is not imported"), "{kept}");
    assert_eq!(
        fs::read_to_string(cache.path(SHA, "16.0.1")).unwrap(),
        record("16.0.1", "a")
    );
    // A record of another build is cached beside it.
    fs::write(&other, record("16.0.2", "b")).unwrap();
    assert!(cache.import(&other).unwrap().starts_with("imported"));
    assert_eq!(
        cache.find(SHA, None).map(|(_, build)| build),
        Some("16.0.2".to_owned())
    );
    // A record that names no build, or no workbook hash, cannot be keyed.
    let unkeyed = dir.join("c.capture.json");
    fs::write(
        &unkeyed,
        format!(
            r#"{{"format": "dnacalc-capture-record", "version": 1, "case": "k", "source": "sha256:{SHA}", "acquisition": {{"kind": "file_cache_replay"}}, "environment": null, "sheets": [], "formulas": [], "observations": []}}"#
        ),
    )
    .unwrap();
    let error = cache.import(&unkeyed).unwrap_err();
    assert!(error.contains("names no Excel build"), "{error}");
    fs::write(
        &unkeyed,
        record("16.0.1", "a").replace(&format!("\"sha256:{SHA}\","), "\"file:x\","),
    )
    .unwrap();
    let error = cache.import(&unkeyed).unwrap_err();
    assert!(error.contains("is not a sha256"), "{error}");
    assert!(cache.import(&dir.join("absent.json")).is_err());
    let _ = fs::remove_dir_all(&dir);
}
