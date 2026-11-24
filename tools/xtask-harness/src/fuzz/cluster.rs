//! Discrepancy clustering, minimal representative selection, and WitnessSet export (`dc-ztvv.21`).
//!
//! Enforces the bank rule (owner 2026-11-21): ONLY inputs where Excel's answer differed from
//! DNA Calc's frozen prediction are exported to `bank-rows/<FUNC>.json` (`dnacalc-witness-set/1`).

use super::rng::{canonical_case_key, parse_hex_bits};
use super::steer::FuzzCase;
use calc_verdict::{Severity, WITNESS_SET_FORMAT, WitnessInvocation, decode_result};
use serde_json::{Map, Value, json};
use std::collections::{BTreeMap, HashSet};

/// True when DNA Calc's witness/invocation path returned a harness or coverage refusal (`fault:*`)
/// rather than a worksheet value or Excel error.
pub fn is_dna_fault(encoded: &str) -> bool {
    encoded.starts_with("fault:")
}

/// Return the fault kind (e.g. `"NotImplemented"`, `"ReferenceRequired"`) for a `fault:*` result.
pub fn dna_fault_kind(encoded: &str) -> Option<&str> {
    encoded.strip_prefix("fault:")
}

/// Verdict of comparing one frozen DNA Calc prediction against Excel's Touchstone reply.
#[derive(Clone, Debug)]
pub enum RowOutcome {
    Matched,
    Discrepancy {
        expected: String,
        severity: Severity,
        ulp: Option<u64>,
    },
    /// DNA Calc's witness/invocation path refused or faulted (`fault:*`): a harness coverage gap,
    /// counted per function in coverage tables and failure totals, never banked or clustered.
    HarnessFault {
        fault: String,
    },
    Unread {
        #[allow(dead_code)]
        reason: String,
    },
    Crashed {
        #[allow(dead_code)]
        detail: String,
    },
}

/// One judged discrepancy row with full provenance for clustering and WitnessSet export.
#[derive(Clone, Debug)]
pub struct DiscrepancyRow {
    pub case: FuzzCase,
    pub expected: String,
    pub severity: Severity,
    pub ulp: Option<u64>,
    pub env_id: String,
    pub cluster_key: String,
}

/// One aggregated discrepancy cluster with minimal and worst-ULP representatives.
#[derive(Clone, Debug)]
pub struct DiscrepancyCluster {
    pub id: String,
    pub key: String,
    pub function: String,
    pub kind: String,
    pub region_group: String,
    pub count: usize,
    pub worst_severity: Severity,
    pub worst_ulp: Option<u64>,
    pub regions: BTreeMap<String, usize>,
    pub minimal_rep: DiscrepancyRow,
    pub worst_rep: DiscrepancyRow,
}

/// Per-region coverage and discrepancy statistics.
#[derive(Clone, Debug, Default)]
pub struct RegionStats {
    pub asked: usize,
    pub matched: usize,
    pub mismatched: usize,
    pub harness_fault: usize,
    pub unread: usize,
    pub crashed: usize,
    pub worst_ulp: Option<u64>,
    pub clusters_seen: usize,
}

impl RegionStats {
    pub fn total_failed(&self) -> usize {
        self.mismatched + self.harness_fault + self.unread + self.crashed
    }
    pub fn pass_rate(&self) -> f64 {
        if self.asked == 0 {
            0.0
        } else {
            (self.matched as f64) * 100.0 / (self.asked as f64)
        }
    }
}

/// Compare Excel's encoded result against DNA Calc's frozen prediction using `calc_verdict::Severity`.
pub fn judge_pair(expected: &str, actual: &str) -> (Severity, Option<u64>) {
    if expected == actual {
        return (Severity::Exact, None);
    }
    match (decode_result(expected), decode_result(actual)) {
        (Ok(e), Ok(a)) => Severity::of(&e, &a),
        _ => (Severity::Structural, None),
    }
}

fn ulp_bucket(severity: Severity, ulp: Option<u64>) -> &'static str {
    match (severity, ulp) {
        (Severity::LastBit, Some(1)) => "1ulp",
        (Severity::LastBit, _) => "2..4ulp",
        (Severity::Numeric, Some(u)) if u <= 32 => "5..32ulp",
        (Severity::Numeric, _) => "33..1024ulp",
        (Severity::Gross, _) => "gross",
        (Severity::Structural, _) => "structural",
        (Severity::Exact, _) => "exact",
    }
}

fn result_kind_label(encoded: &str) -> String {
    if encoded.starts_with("0x") {
        "number".to_owned()
    } else if let Some(err) = encoded.strip_prefix("error:") {
        format!("error:{err}")
    } else if encoded.starts_with("logical:") {
        "logical".to_owned()
    } else if encoded.starts_with("text:") {
        "text".to_owned()
    } else if let Some(fault) = encoded.strip_prefix("fault:") {
        format!("fault:{fault}")
    } else {
        "other".to_owned()
    }
}

fn normalize_region_group(region: &str) -> String {
    if let Some(rest) = region.strip_prefix("densify:") {
        return rest.to_owned();
    }
    if region.starts_with("boundary:") {
        return "boundary".to_owned();
    }
    region.to_owned()
}

/// Compute a semantic feature signature to separate genuinely distinct root causes within the same
/// severity/kind bucket (e.g. RATE flat-secant vs negative-period overflow vs 200-iter cutoff vs
/// tiny-cashflow underflow vs ordinary negative-fraction POWER rounding).
pub fn feature_signature(function: &str, case: &FuzzCase, expected: &str) -> String {
    let args = &case.args;
    let trace = &case.trace;
    match function {
        "RATE" => {
            let nper = args.first().and_then(parse_hex_bits).map(f64::from_bits);
            let nper_cls = match nper {
                None => "nonnum_nper",
                Some(n) if n == 0.0 => "nper_zero",
                Some(n) if n < -1e4 => "nper_neg_huge",
                Some(n) if n < 0.0 && n.fract() != 0.0 => "nper_neg_frac",
                Some(n) if n < 0.0 => "nper_neg_int",
                Some(n) if n.fract() != 0.0 => "nper_pos_frac",
                Some(_) => "nper_pos_int",
            };
            let timing = args
                .get(4)
                .and_then(parse_hex_bits)
                .map(f64::from_bits)
                .is_some_and(|t| t != 0.0);
            let timing_cls = if timing { "beg" } else { "end" };
            let mag = args
                .iter()
                .take(4)
                .filter_map(parse_hex_bits)
                .map(|b| f64::from_bits(b).abs())
                .filter(|&v| v > 0.0 && v.is_finite())
                .fold((f64::INFINITY, 0.0_f64), |(mn, mx), v| (mn.min(v), mx.max(v)));
            let scale_cls = if mag.0 < 1e-280 {
                "tiny_underflow"
            } else if mag.1 > 1e280 {
                "huge_overflow"
            } else if expected
                .starts_with("0x")
                && parse_hex_bits(&Value::String(expected.to_owned()))
                    .is_some_and(|b| f64::from_bits(b).abs() >= 536_870_912.0)
                || trace.has_bit(26)
                || trace.has_bit(27)
            {
                "huge_root"
            } else {
                "normal_scale"
            };
            let solver_cls = if trace.has_bit(14) || trace.has_bit(15) {
                "neg_period_overflow"
            } else if trace.has_bit(11) {
                "double_flat"
            } else if trace.has_bit(6) {
                "flat_nudge"
            } else if trace.has_bit(12) {
                "daz_div0"
            } else if trace.iterations >= 200 {
                "iter200"
            } else if trace.has_bit(0) || trace.has_bit(25) {
                "neg_one_edge"
            } else {
                "regular_secant"
            };
            format!("{nper_cls}|{timing_cls}|{scale_cls}|{solver_cls}")
        }
        "IRR" => {
            let retry = if trace.has_bit(2) {
                "r0"
            } else if trace.has_bit(3) {
                "r1_0.1"
            } else if trace.has_bit(4) {
                "r2_-0.1"
            } else if trace.has_bit(5) {
                "r3_2/3"
            } else if trace.has_bit(6) {
                "all_fail"
            } else {
                "early_err"
            };
            let iter_cls = if trace.iterations >= 200 {
                "iter200"
            } else if trace.has_bit(10) || trace.has_bit(11) || trace.has_bit(12) {
                "flat_nudge"
            } else {
                "regular"
            };
            format!("arity={}|{retry}|{iter_cls}", args.len())
        }
        "YIELD" => {
            let period_cls = if trace.has_bit(1) {
                "n1"
            } else if trace.has_bit(13) {
                "iter100"
            } else if trace.has_bit(7) || trace.has_bit(8) {
                "neg_base"
            } else if trace.has_bit(0) {
                "domain_err"
            } else {
                "multi"
            };
            let basis = args
                .get(6)
                .and_then(parse_hex_bits)
                .map(|b| f64::from_bits(b).trunc() as i64)
                .unwrap_or(0);
            format!("{period_cls}|basis={basis}")
        }
        _ => {
            let has_nonnum = args.iter().any(|a| parse_hex_bits(a).is_none() && !a.is_array());
            format!(
                "arity={}|nonnum={has_nonnum}|iter={}",
                args.len(),
                if trace.iterations >= 100 { "high" } else { "low" }
            )
        }
    }
}

/// Build the canonical cluster key for one discrepancy.
pub fn cluster_key_for(
    function: &str,
    case: &FuzzCase,
    expected: &str,
    severity: Severity,
    ulp: Option<u64>,
) -> String {
    let e_kind = result_kind_label(expected);
    let a_kind = result_kind_label(&case.prediction);
    let kind = if e_kind != a_kind {
        format!("{e_kind}_vs_{a_kind}")
    } else {
        format!("{}:{}", severity.name(), ulp_bucket(severity, ulp))
    };
    let feat = feature_signature(function, case, expected);
    format!("{kind}|{feat}")
}

/// Complexity score for choosing a minimal representative (lower = simpler / cleaner inputs).
pub(crate) fn arg_complexity(value: &Value) -> u64 {
    match value {
        Value::String(s) => {
            if let Some(bits) = parse_hex_bits(value) {
                let v = f64::from_bits(bits);
                let mantissa = bits & ((1u64 << 52) - 1);
                let pop = u64::from(mantissa.count_ones());
                let exp = (((bits >> 52) & 0x7ff) as i64 - 1023).unsigned_abs();
                let sign_pen = u64::from(v < 0.0);
                pop * 10 + exp / 4 + sign_pen
            } else {
                100 + s.len() as u64
            }
        }
        Value::Array(items) => {
            items.len() as u64 * 20 + items.iter().map(arg_complexity).sum::<u64>()
        }
        Value::Null => 5,
        Value::Bool(_) => 10,
        _ => 150,
    }
}

fn case_complexity(case: &FuzzCase) -> u64 {
    (case.args.len() as u64) * 25 + case.args.iter().map(arg_complexity).sum::<u64>()
}

/// Group all discrepancy rows into sorted clusters with minimal & worst-ULP representatives.
/// Defense in depth: DNA-side `fault:*` rows are never clustered as Excel discrepancies.
pub fn build_clusters(function: &str, rows: &[DiscrepancyRow]) -> Vec<DiscrepancyCluster> {
    let mut grouped: BTreeMap<String, Vec<&DiscrepancyRow>> = BTreeMap::new();
    for row in rows {
        if is_dna_fault(&row.case.prediction) {
            continue;
        }
        grouped.entry(row.cluster_key.clone()).or_default().push(row);
    }
    let mut clusters: Vec<DiscrepancyCluster> = Vec::with_capacity(grouped.len());
    for (key, members) in grouped {
        let mut regions: BTreeMap<String, usize> = BTreeMap::new();
        let mut worst_severity = Severity::Exact;
        let mut worst_ulp: Option<u64> = None;
        let mut minimal_rep = members[0];
        let mut min_score = case_complexity(&minimal_rep.case);
        let mut worst_rep = members[0];
        for &m in &members {
            *regions.entry(m.case.region.clone()).or_insert(0) += 1;
            if m.severity > worst_severity {
                worst_severity = m.severity;
            }
            if let Some(u) = m.ulp {
                worst_ulp = Some(worst_ulp.map_or(u, |prev| prev.max(u)));
            }
            let score = case_complexity(&m.case);
            if score < min_score
                || (score == min_score && m.case.id < minimal_rep.case.id)
            {
                min_score = score;
                minimal_rep = m;
            }
            if (m.severity, m.ulp.unwrap_or(0)) > (worst_rep.severity, worst_rep.ulp.unwrap_or(0)) {
                worst_rep = m;
            }
        }
        let primary_region = regions
            .iter()
            .max_by_key(|&(_, c)| *c)
            .map(|(r, _)| normalize_region_group(r))
            .unwrap_or_else(|| "unknown".to_owned());
        let kind = key.split('|').next().unwrap_or("other").to_owned();
        clusters.push(DiscrepancyCluster {
            id: String::new(),
            key,
            function: function.to_owned(),
            kind,
            region_group: primary_region,
            count: members.len(),
            worst_severity,
            worst_ulp,
            regions,
            minimal_rep: minimal_rep.clone(),
            worst_rep: worst_rep.clone(),
        });
    }
    // Sort: structural/gross first, then by count descending, then key for stability.
    clusters.sort_by(|a, b| {
        b.worst_severity
            .cmp(&a.worst_severity)
            .then_with(|| b.count.cmp(&a.count))
            .then_with(|| a.key.cmp(&b.key))
    });
    for (i, c) in clusters.iter_mut().enumerate() {
        c.id = format!("C{:02}", i + 1);
    }
    clusters
}

impl DiscrepancyCluster {
    pub fn to_json(&self) -> Value {
        json!({
            "id": self.id,
            "key": self.key,
            "external_ref": super::coverage::cluster_external_ref(&self.function, &self.key),
            "function": self.function,
            "kind": self.kind,
            "region_group": self.region_group,
            "count": self.count,
            "worst_severity": self.worst_severity.name(),
            "worst_ulp": self.worst_ulp,
            "regions": self.regions,
            "minimal_rep": {
                "id": self.minimal_rep.case.id,
                "region": self.minimal_rep.case.region,
                "args": self.minimal_rep.case.args,
                "excel": self.minimal_rep.expected,
                "calc_func": self.minimal_rep.case.prediction,
                "severity": self.minimal_rep.severity.name(),
                "ulp": self.minimal_rep.ulp,
                "iterations": self.minimal_rep.case.trace.iterations,
                "route_mask": format!("0x{:x}", self.minimal_rep.case.trace.route_mask),
            },
            "worst_rep": {
                "id": self.worst_rep.case.id,
                "region": self.worst_rep.case.region,
                "args": self.worst_rep.case.args,
                "excel": self.worst_rep.expected,
                "calc_func": self.worst_rep.case.prediction,
                "severity": self.worst_rep.severity.name(),
                "ulp": self.worst_rep.ulp,
            },
        })
    }
}

/// Build a `dnacalc-witness-set/1` JSON document compatible with both
/// `calc_verdict::WitnessSet::decode` and `CalcFunc-Evidence/harvest-bank-rows.py:lane_discrepancies`.
///
/// Only discrepancy rows (`expected != calc_func`) may be passed here (the bank rule).
pub fn encode_witness_set_json(
    function: &str,
    lane: &str,
    source_path: &str,
    selection: &str,
    timestamp_utc: &str,
    envs: &BTreeMap<String, Value>,
    invocation: &WitnessInvocation,
    rows: &[&DiscrepancyRow],
) -> Result<String, String> {
    // Verify bank rule invariant: every exported row must actually differ!
    // Also deduplicate by canonical (function, args) key so bank-rows never receives duplicate rows.
    let mut seen_canonical: HashSet<String> = HashSet::with_capacity(rows.len());
    let mut unique_rows: Vec<&DiscrepancyRow> = Vec::with_capacity(rows.len());
    for &row in rows {
        if is_dna_fault(&row.case.prediction) {
            return Err(format!(
                "refusing to export DNA-side harness fault row {} ({}) into discrepancy WitnessSet",
                row.case.id, row.case.prediction
            ));
        }
        if row.expected == row.case.prediction {
            return Err(format!(
                "refusing to export matching row {} into discrepancy WitnessSet",
                row.case.id
            ));
        }
        if seen_canonical.insert(canonical_case_key(function, &row.case.args)) {
            unique_rows.push(row);
        }
    }
    let mut env_to_rid: BTreeMap<String, String> = BTreeMap::new();
    let mut readings: Vec<Value> = Vec::new();
    for row in &unique_rows {
        if !env_to_rid.contains_key(&row.env_id) {
            let rid = format!("r{}", readings.len());
            let env = envs.get(&row.env_id);
            let excel_build = env
                .and_then(|e| {
                    e.get("file_version")
                        .and_then(Value::as_str)
                        .filter(|s| !s.is_empty())
                        .or_else(|| e.get("build").and_then(Value::as_str).filter(|s| !s.is_empty()))
                })
                .unwrap_or("unknown");
            let cpu = env
                .and_then(|e| {
                    e.get("cpu")
                        .and_then(Value::as_str)
                        .filter(|s| !s.is_empty())
                        .or_else(|| e.get("cpu_id").and_then(Value::as_str).filter(|s| !s.is_empty()))
                })
                .unwrap_or("unknown");
            let bitness = env
                .and_then(|e| e.get("machine"))
                .and_then(Value::as_str)
                .map(|m| if m.contains("64") { "64-bit" } else { "32-bit" })
                .unwrap_or("64-bit");
            let mut reading = Map::new();
            reading.insert("id".into(), json!(rid));
            reading.insert("source".into(), json!(source_path));
            reading.insert("excel_build".into(), json!(excel_build));
            reading.insert("platform".into(), json!("Windows"));
            reading.insert("bitness".into(), json!(bitness));
            reading.insert("cpu".into(), json!(cpu));
            reading.insert("env_id".into(), json!(row.env_id));
            reading.insert("lane".into(), json!(lane));
            reading.insert("timestamp".into(), json!(timestamp_utc));
            reading.insert("invocation".into(), invocation.to_json());
            readings.push(Value::Object(reading));
            env_to_rid.insert(row.env_id.clone(), rid);
        }
    }
    let mut witnesses: Vec<Value> = Vec::with_capacity(unique_rows.len());
    for (idx, row) in unique_rows.iter().enumerate() {
        let rid = &env_to_rid[&row.env_id];
        witnesses.push(json!({
            "id": format!("{lane}-{function}-{:05}", idx + 1),
            "reading": rid,
            "args": row.case.args,
            "expected_bits": row.expected,
            "calc_func": row.case.prediction,
        }));
    }
    let doc = json!({
        "format": WITNESS_SET_FORMAT,
        "function": function,
        "lane": lane,
        "selection": selection,
        "readings": readings,
        "witnesses": witnesses,
    });
    let text = serde_json::to_string_pretty(&doc).map_err(|e| e.to_string())? + "\n";
    // Validate round-trip through the one verdict WitnessSet decoder.
    calc_verdict::WitnessSet::decode(text.as_bytes())
        .map_err(|e| format!("generated WitnessSet failed decode validation: {e}"))?;
    Ok(text)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fuzz::rng::hex_f64;
    use crate::fuzz::steer::touchstone_bulk_context;
    use calc_func::trace::RouteTrace;

    #[test]
    fn witness_set_export_rejects_matching_rows_and_roundtrips_discrepancies() {
        let mut envs = BTreeMap::new();
        envs.insert(
            "b9d22089d2f5".to_owned(),
            json!({
                "file_version": "16.0.20625.20108",
                "cpu": "AMD Ryzen 7 4800H with Radeon Graphics",
                "machine": "x86-64"
            }),
        );
        let row = DiscrepancyRow {
            case: FuzzCase {
                id: "fuzz-RATE-r00-000001".to_owned(),
                region: "rate_near_flat_and_regimes".to_owned(),
                round: 0,
                args: vec![hex_f64(10.0), hex_f64(-120.0), hex_f64(1000.0)],
                prediction: "error:Num".to_owned(),
                trace: RouteTrace::default(),
                outcome_class: "err:Num".to_owned(),
            },
            expected: "0x3fa1b74af437cf1e".to_owned(),
            severity: Severity::Structural,
            ulp: None,
            env_id: "b9d22089d2f5".to_owned(),
            cluster_key: "number_vs_error:Num|nper_pos_int|end|normal_scale|regular_secant"
                .to_owned(),
        };
        let encoded = encode_witness_set_json(
            "RATE",
            "sf1",
            "sf1/rate-sweep/spec.json",
            "discrepancies only",
            "2026-11-23T22:30:00Z",
            &envs,
            &touchstone_bulk_context(),
            &[&row],
        )
        .expect("valid WitnessSet");
        let parsed = calc_verdict::WitnessSet::decode(encoded.as_bytes()).unwrap();
        assert_eq!(parsed.function, "RATE");
        assert_eq!(parsed.witnesses.len(), 1);
        assert_eq!(parsed.witnesses[0].calc_func.as_deref(), Some("error:Num"));
        assert_eq!(parsed.readings.len(), 1);
        assert_eq!(parsed.readings[0].excel_build.as_deref(), Some("16.0.20625.20108"));
        assert_eq!(
            parsed.readings[0].cpu.as_deref(),
            Some("AMD Ryzen 7 4800H with Radeon Graphics")
        );
        assert_eq!(parsed.readings[0].bitness.as_deref(), Some("64-bit"));

        // Verify fallback to `build`/`cpu_id` and 32-bit `machine`.
        let mut envs_32 = BTreeMap::new();
        envs_32.insert(
            "e32".to_owned(),
            json!({
                "file_version": "",
                "build": "20625",
                "cpu_id": "x86 Family 6",
                "machine": "x86"
            }),
        );
        let mut row_32 = row.clone();
        row_32.env_id = "e32".to_owned();
        let encoded_32 = encode_witness_set_json(
            "RATE",
            "sf1",
            "sf1/rate-sweep/spec.json",
            "discrepancies only",
            "2026-11-23T22:30:00Z",
            &envs_32,
            &touchstone_bulk_context(),
            &[&row_32],
        )
        .expect("32-bit fallback WitnessSet");
        let parsed_32 = calc_verdict::WitnessSet::decode(encoded_32.as_bytes()).unwrap();
        assert_eq!(parsed_32.readings[0].excel_build.as_deref(), Some("20625"));
        assert_eq!(parsed_32.readings[0].cpu.as_deref(), Some("x86 Family 6"));
        assert_eq!(parsed_32.readings[0].bitness.as_deref(), Some("32-bit"));

        let mut matching = row.clone();
        matching.expected = matching.case.prediction.clone();
        assert!(
            encode_witness_set_json(
                "RATE",
                "sf1",
                "sf1/rate-sweep/spec.json",
                "bad",
                "2026-11-23T22:30:00Z",
                &envs,
                &touchstone_bulk_context(),
                &[&matching],
            )
            .is_err()
        );

        // DNA-side harness faults (`fault:*`) must NEVER be exported to bank-rows or clustered.
        for fault in [
            "fault:ReferenceRequired",
            "fault:NotImplemented",
            "fault:ResultShape",
            "fault:WitnessReferencePosition",
        ] {
            let mut fault_row = row.clone();
            fault_row.case.prediction = fault.to_owned();
            assert!(
                encode_witness_set_json(
                    "COUNTIF",
                    "sf1",
                    "sf1/countif/spec.json",
                    "bad",
                    "2026-11-24T12:00:00Z",
                    &envs,
                    &touchstone_bulk_context(),
                    &[&fault_row],
                )
                .is_err(),
                "expected {fault} to be refused from discrepancy WitnessSet"
            );
            assert!(
                build_clusters("COUNTIF", &[fault_row]).is_empty(),
                "expected {fault} to be excluded from discrepancy clusters"
            );
        }

        // Duplicate canonical inputs must be deduplicated so bank-rows never contains the same row twice.
        let mut dup = row.clone();
        dup.case.id = "fuzz-RATE-b01-000099".to_owned();
        let deduped = encode_witness_set_json(
            "RATE",
            "sf1",
            "sf1/rate-sweep/spec.json",
            "discrepancies only",
            "2026-11-23T22:30:00Z",
            &envs,
            &touchstone_bulk_context(),
            &[&row, &dup],
        )
        .expect("deduped WitnessSet");
        let parsed_dedup = calc_verdict::WitnessSet::decode(deduped.as_bytes()).unwrap();
        assert_eq!(
            parsed_dedup.witnesses.len(),
            1,
            "duplicate canonical inputs should collapse to a single bank row"
        );
    }
}
