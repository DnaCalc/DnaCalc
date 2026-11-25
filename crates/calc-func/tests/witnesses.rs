//! The pinned witness sets (bead dc-ymjf.2): per function, Excel's readings drawn from the banks
//! of the evidence repository by `cargo xtask witness pin`, judged bit for bit through
//! `calc_func::invoke` under the application's numeric mode. A row calc-func matched when it was
//! pinned must still equal Excel's reading; a row it did not match carries the result calculated
//! then (`calc_func`) and must still produce exactly that. So a regression and a gain both fail
//! here, and a fix updates the pin (`cargo xtask witness pin --func <F>`) in the same commit, where
//! the diff of `crates/calc-func/witness-status.tsv` shows it.

#[path = "witnesses/index.rs"]
mod index;

#[path = "witnesses/call.rs"]
mod witness_call;

use crate::x87_cpu;
use calc_verdict::WitnessSet;

/// Excel 16.0.20527, ax2/f-density-masses-answers.jsonl row 63.
#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn f_dist_flushes_subnormal_density() {
    let args = serde_json::json!([
        "0x40368acbfa739b28", "0x4058000000000000", "0x40b6cd0000000000", false
    ]);
    assert_eq!(
        witness_call::calculate("F.DIST", args.as_array().unwrap(), None, None, true),
        "0x0000000000000000"
    );
}

/// Excel 16.0.20527 from ax2, nx2 and cb5's retained discrepancy sets.
#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn beta_dist_flushes_subnormal_results() {
    for args in [
        serde_json::json!(["0x0010000000000000", "0x3fefffffffffffff", "0x3da8089f6e924e68", true]),
        serde_json::json!(["0x3f8f709c24b9fa3c", "0x4068ab5b2480b046", "0x4041ab5fe7a9160a", true]),
        serde_json::json!(["0x3e923126e978d4fe", "0x404ad9999999999a", "0x4047a66666666666", false]),
    ] {
        assert_eq!(
            witness_call::calculate("BETA.DIST", args.as_array().unwrap(), None, None, true),
            "0x0000000000000000",
            "{args}"
        );
    }
}

/// Excel 16.0.20527, ax2/beta-inverse-boundary-cdf-answers.jsonl row 262.
#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn beta_dist_cdf_at_minimum_normal_coordinate() {
    let args = serde_json::json!([
        "0x0010000000000000", "0x3e33fc282eeff1bc", "0x4000000000000000", true
    ]);
    assert_eq!(
        witness_call::calculate("BETA.DIST", args.as_array().unwrap(), None, None, true),
        "0x3feffff91f729bc4"
    );
}

/// Touchstone Excel 16.0.20625.20108, dc-j9k.225. An odd significand loses a bit
/// if the internal logarithm stores half the input before doubling it.
#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn distribution_log_preserves_smallest_normal_inputs() {
    for (function, args, expected) in [
        (
            "GAMMA.INV",
            serde_json::json!(["0x0010000000000001", "0x3ff0000000000000", "0x7fe0000000000000"]),
            "0x4000000000000001",
        ),
        (
            "BETA.DIST",
            serde_json::json!(["0x0010000000000001", "0x3ff0000000000000", "0x4008000000000000", true]),
            "0x0028000000000002",
        ),
    ] {
        assert_eq!(
            witness_call::calculate(function, args.as_array().unwrap(), None, None, true),
            expected,
            "{function} {args}"
        );
    }
}

#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn every_pinned_witness_row_calculates_as_pinned() {
    assert!(!index::SETS.is_empty(), "no pinned witness sets");
    let mut rows = 0;
    let mut failures = Vec::new();
    for (file, text) in index::SETS {
        let set = WitnessSet::decode(text.as_bytes()).unwrap_or_else(|e| panic!("{file}: {e}"));
        assert_eq!(
            *file,
            format!("{}.json", set.function),
            "{file} names its function"
        );
        for row in &set.witnesses {
            rows += 1;
            let context = set
                .readings
                .iter()
                .find(|source| source.id == row.reading)
                .and_then(|source| source.invocation.as_ref());
            let actual =
                witness_call::calculate(&set.function, &row.args, row.result_index, context, true);
            let pinned = row.calc_func.as_deref().unwrap_or(&row.expected);
            let cpu_row = match row.args.as_slice() {
                [arg] => arg
                    .as_str()
                    .and_then(|s| s.strip_prefix("0x"))
                    .and_then(|s| u64::from_str_radix(s, 16).ok())
                    .and_then(|input| x87_cpu::find(&set.function, input)),
                _ => None,
            };
            // Excel itself on Intel has not been measured for these inputs. Check the
            // hardware result for this CPUID vendor without changing the Excel reading.
            let want = if let Some(cpu_row) = cpu_row {
                assert_eq!(
                    pinned,
                    format!(
                        "0x{:016x}",
                        cpu_row.for_vendor(x87_cpu::AMD_RYZEN_7_4800H.vendor).bits
                    ),
                    "original pin"
                );
                format!("0x{:016x}", cpu_row.expected_bits())
            } else {
                pinned.to_owned()
            };
            if actual != want {
                let what = if actual == row.expected {
                    "now matches Excel (a gain: re-pin)"
                } else if row.calc_func.is_none() {
                    "no longer matches Excel (a regression)"
                } else {
                    "changed and still differs from Excel"
                };
                failures.push(format!(
                    "{} {} {:?}: Excel {}, pinned {want}, calculated {actual}: {what}",
                    set.function, row.id, row.args, row.expected
                ));
            }
        }
    }
    assert!(
        failures.is_empty(),
        "{} of {rows} pinned witness rows changed:\n{}",
        failures.len(),
        failures
            .iter()
            .take(40)
            .cloned()
            .collect::<Vec<_>>()
            .join("\n")
    );
}

/// The retained W111 typed run explicitly recorded CompatibilityVersion2. LEN's raw
/// Value2 answer for w111typed-00072 is3; this exercises the shared native/pinned-bank caller.
#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn recorded_compatibility_version_reaches_the_witness_call() {
    let context = calc_verdict::WitnessInvocation {
        compatibility_version: Some("2".into()),
        ..Default::default()
    };
    let args = [serde_json::json!("A😀B")];
    assert_eq!(
        witness_call::calculate("LEN", &args, None, Some(&context), true),
        "0x4008000000000000"
    );
    assert_eq!(
        witness_call::calculate("LEN", &args, None, None, true),
        "0x4010000000000000"
    );
}

/// Pinned minimal representatives for every discrepancy cluster discovered by `cargo xtask fuzz`
/// on live Excel 16.0.20625.20108 (AMD Ryzen 7 4800H, lane sf2, bead dc-ztvv.21). Fixed clusters
/// (`RATE` R1/R3 after 3766bca80) omit `calc_func` and must match Excel bit-for-bit; open Carbon
/// clusters (`dc-j9k.79`, `dc-j9k.97`..`dc-j9k.105`) carry `calc_func` so both regressions and
/// future gains fail here and prompt re-pinning.
#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn fuzz_cluster_representatives_calculate_as_pinned() {
    const SETS: &[(&str, &str)] = &[
        (
            "RATE.json",
            include_str!("witnesses/fuzz_representatives/RATE.json"),
        ),
        (
            "YIELD.json",
            include_str!("witnesses/fuzz_representatives/YIELD.json"),
        ),
        (
            "IRR.json",
            include_str!("witnesses/fuzz_representatives/IRR.json"),
        ),
    ];
    let mut rows = 0;
    let mut failures = Vec::new();
    for (file, text) in SETS {
        let set = WitnessSet::decode(text.as_bytes()).unwrap_or_else(|e| panic!("{file}: {e}"));
        assert_eq!(*file, format!("{}.json", set.function));
        for row in &set.witnesses {
            rows += 1;
            let context = set
                .readings
                .iter()
                .find(|source| source.id == row.reading)
                .and_then(|source| source.invocation.as_ref());
            let actual =
                witness_call::calculate(&set.function, &row.args, row.result_index, context, true);
            let want = row.calc_func.as_deref().unwrap_or(&row.expected);
            if actual != want {
                let what = if actual == row.expected {
                    "now matches Excel (a gain: drop calc_func from this representative)"
                } else if row.calc_func.is_none() {
                    "no longer matches Excel (a regression)"
                } else {
                    "changed and still differs from Excel"
                };
                failures.push(format!(
                    "{} {} {:?}: Excel {}, pinned {want}, calculated {actual}: {what}",
                    set.function, row.id, row.args, row.expected
                ));
            }
        }
    }
    assert!(rows >= 25, "expected pinned cluster representatives");
    assert!(
        failures.is_empty(),
        "{} of {rows} fuzz cluster representatives changed:\n{}",
        failures.len(),
        failures.join("\n")
    );
}
