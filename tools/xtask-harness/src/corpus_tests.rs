//! Unit tests of `cargo xtask corpus` (`corpus.rs`): its arguments, the workbooks it finds, and
//! the capture cache's keys, lookups and imports. None of them starts Excel or a compare process.

use super::*;

#[test]
fn a_changed_batch_source_is_refused_before_a_child_is_started() {
    let root = scratch("changed-after-inventory");
    let path = root.join("case.xlsx");
    fs::write(&path, b"old source").unwrap();
    let (entries, _) = crate::corpus_batch::select(folder_workbooks(&root).unwrap(), None);
    let args = parse_args(&[root.to_string_lossy().into_owned(), "--dna-only".into()]).unwrap();
    let mut run = Run {
        args: &args,
        cache: Cache {
            root: root.join("cache"),
        },
        build: None,
        program_digest: String::new(),
    };
    fs::write(&path, b"new source").unwrap();
    let report = one_workbook(
        &mut run,
        &root.join("no-such-executable"),
        &entries[0],
        &root.join("out"),
    );
    assert_eq!(report.failure_key.as_deref(), Some("SourceChanged"));
    assert_eq!(report.outcome.as_ref().unwrap_err().stage, "read");
    assert!(report.inventory_formula_cells.is_none());
    assert_eq!(
        report.sha256.as_deref(),
        Some(calc_verdict::sha256_hex(b"new source").as_str())
    );
    assert!(
        !root.join("out").exists(),
        "no source-census or comparison child ran"
    );
}

#[test]
fn a_linux_cache_lookup_keeps_the_recorded_iteration_limits() {
    let cache = Cache {
        root: scratch("linux-policy"),
    };
    let plain = CaptureRecord::decode(record("16.0.1", "plain").as_bytes()).unwrap();
    let mut encoded: serde_json::Value =
        serde_json::from_str(&record("16.0.2", "iteration")).unwrap();
    encoded["environment"]["settings"]["calculation"] = json!("manual");
    encoded["environment"]["settings"]["iteration"] = json!("true");
    encoded["environment"]["settings"]["max_iterations"] = json!("5");
    encoded["environment"]["settings"]["max_change"] = json!("3fc0000000000000");
    let iterating =
        CaptureRecord::decode(serde_json::to_string(&encoded).unwrap().as_bytes()).unwrap();
    for r in [&plain, &iterating] {
        let (hash, key) = record_key(r).unwrap();
        cache
            .store(&hash, &key, r.encode().unwrap().as_bytes())
            .unwrap();
    }
    let policy = record_policy(&iterating);
    assert!(
        cache
            .find_policy(SHA, &policy)
            .unwrap()
            .1
            .contains("iteration-5-3fc0000000000000")
    );
    assert_eq!(
        cache
            .find_policy(SHA, &CalculationPolicy::default())
            .unwrap()
            .1,
        "16.0.1"
    );
    let mut other = policy;
    other.max_iterations = 6;
    assert!(cache.find_policy(SHA, &other).is_none());
}

#[test]
fn recursive_inventory_deduplicates_content_before_stable_sharding() {
    let root = scratch("recursive-shards");
    fs::create_dir_all(root.join("nested")).unwrap();
    for (name, bytes) in [
        ("a.xlsx", b"same".as_slice()),
        ("nested/b.xlsx", b"same".as_slice()),
        ("nested/c.xlsm", b"other".as_slice()),
    ] {
        fs::write(root.join(name), bytes).unwrap();
    }
    let entries = folder_tree(&root, true).unwrap();
    assert_eq!(entries.len(), 3);
    let (all, manifest) = crate::corpus_batch::select(entries.clone(), None);
    assert_eq!(all.len(), 2);
    assert_eq!(manifest["unique_content"], 2);
    assert_eq!(
        manifest["files"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|f| f["duplicate_of"].is_string())
            .count(),
        1
    );
    let mut selected = BTreeMap::new();
    for index in 0..5 {
        let (part, _) = crate::corpus_batch::select(entries.clone(), Some((index, 5)));
        for entry in part {
            assert!(selected.insert(entry.path, index).is_none());
        }
    }
    assert_eq!(selected.len(), all.len());
    assert_eq!(folder_tree(&root, false).unwrap().len(), 1);
    for bad in ["0/0", "1/1", "-1/2", "0", "2/x"] {
        assert!(parse_args(&args(&format!("dir --shard {bad}"))).is_err());
    }
}

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
            recursive: false,
            shard: None,
            resume: false,
            minimize: false,
            dna_only: false,
            merges: Vec::new(),
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
    assert_eq!(names, ["a.XLSM", "b.xlsx", "c.xls"]);
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

/// A record taken under another calculation policy than Excel's default (a workbook that
/// iterates, captured under manual calculation with its iteration settings, dc-10y) is cached by
/// its build and that policy, never found by the build alone; one taken under the default policy
/// is keyed by the build as before.
#[test]
fn the_cache_keys_a_record_by_its_calculation_policy() {
    let default = CalculationPolicy::default();
    assert_eq!(policy_key("16.0.1", &default), "16.0.1");
    let iterating = CalculationPolicy {
        mode: calc_oracle_excel::CalculationMode::Manual,
        iteration: true,
        max_iterations: 50,
        max_change: 0.0001,
    };
    let key = policy_key("16.0.1", &iterating);
    assert_eq!(key, "16.0.1_policy-manual-iteration-50-3f1a36e2eb1c432d");

    let dir = scratch("policy");
    let cache = Cache {
        root: dir.join("cache"),
    };
    let source = dir.join("i.capture.json");
    let settings = r#"{"date1904": "false"}"#;
    let iterated = r#"{"date1904": "false", "calculation": "manual", "iteration": "true", "max_iterations": "50", "max_change": "3f1a36e2eb1c432d"}"#;
    fs::write(&source, record("16.0.1", "i").replace(settings, iterated)).unwrap();
    assert!(cache.import(&source).unwrap().starts_with("imported"));
    assert_eq!(cache.find(SHA, Some("16.0.1")), None);
    assert_eq!(cache.find(SHA, None), None);
    assert_eq!(
        cache.find(SHA, Some(&key)).map(|(_, build)| build),
        Some(key.clone())
    );
    let _ = fs::remove_dir_all(&dir);
}
