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
