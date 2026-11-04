//! The pinned witness sets (bead dc-ymjf.2): per function, Excel's readings drawn from the banks
//! of the evidence repository by `cargo xtask witness pin`, judged bit for bit through
//! `calc_func::invoke` under the application's numeric mode. A row calc-func matched when it was
//! pinned must still equal Excel's reading; a row it did not match carries the result calculated
//! then (`calc_func`) and must still produce exactly that. So a regression and a gain both fail
//! here, and a fix updates the pin (`cargo xtask witness pin --func <F>`) in the same commit, where
//! the diff of `crates/calc-func/witness-status.tsv` shows it.

#[path = "witnesses/index.rs"]
mod index;

#[path = "f1_support/x87_cpu.rs"]
mod x87_cpu;

#[path = "witnesses/call.rs"]
mod witness_call;

use calc_verdict::WitnessSet;

#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn every_pinned_witness_row_calculates_as_pinned() {
    assert!(!index::SETS.is_empty(), "no pinned witness sets");
    let mut rows = 0;
    let mut failures = Vec::new();
    for (file, text) in index::SETS {
        let set = WitnessSet::decode(text.as_bytes()).unwrap_or_else(|e| panic!("{file}: {e}"));
        assert_eq!(*file, format!("{}.json", set.function), "{file} names its function");
        for row in &set.witnesses {
            rows += 1;
            let context = set.readings.iter().find(|source| source.id == row.reading)
                .and_then(|source| source.invocation.as_ref());
            let actual = witness_call::calculate(&set.function, &row.args, row.result_index, context, true);
            let pinned = row.calc_func.as_deref().unwrap_or(&row.expected);
            let cpu_row = match row.args.as_slice() {
                [arg] => arg.as_str().and_then(|s| s.strip_prefix("0x"))
                    .and_then(|s| u64::from_str_radix(s, 16).ok())
                    .and_then(|input| x87_cpu::find(&set.function, input)),
                _ => None,
            };
            // Excel itself on Intel has not been measured for these inputs. Check the
            // hardware result for this CPUID vendor without changing the Excel reading.
            let want = if let Some(cpu_row) = cpu_row {
                assert_eq!(pinned, format!("0x{:016x}", cpu_row.for_vendor("AuthenticAMD").bits), "original pin");
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
                    set.function,
                    row.id,
                    row.args,
                    row.expected
                ));
            }
        }
    }
    assert!(
        failures.is_empty(),
        "{} of {rows} pinned witness rows changed:\n{}",
        failures.len(),
        failures.iter().take(40).cloned().collect::<Vec<_>>().join("\n")
    );
}
