//! Queue the existing workers in the lane's owned, long-lived Excel session.
//! The host's receipt must prove the same recorded baseline before and after the job.

use crate::CaptureError;
use crate::isolation::{ReportedExcel, WorkerRun};
use serde_json::{Value, json};
use std::path::Path;
use std::process::Command;
use std::time::Duration;

pub(crate) fn run_worker(
    script: &Path,
    request: &Path,
    result: &Path,
    work: &Path,
    timeout: Duration,
    environment: &str,
) -> Result<WorkerRun, CaptureError> {
    let client = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tools/excel-session/submit.ps1");
    let parameters = json!({"Request": request, "Result": result}).to_string();
    let output = Command::new("pwsh.exe")
        .args(["-NoLogo", "-NoProfile", "-NonInteractive", "-STA", "-File"])
        .arg(&client)
        .arg("-Script")
        .arg(script)
        .arg("-Parameters")
        .arg(parameters)
        .arg("-WorkingDirectory")
        .arg(work)
        .arg("-Environment")
        .arg(environment)
        .arg("-TimeoutSeconds")
        .arg(timeout.as_secs().max(1).to_string())
        .output()
        .map_err(|e| CaptureError::Setup {
            message: format!("cannot submit Excel session job: {e}"),
        })?;
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr).into_owned();
    let receipt_path = stdout
        .lines()
        .find_map(|line| line.strip_prefix("dnacalc-excel-session/1 receipt "));
    let receipt: Option<Value> = receipt_path
        .and_then(|p| std::fs::read(p).ok())
        .and_then(|b| serde_json::from_slice(&b).ok());
    let Some(receipt) = receipt else {
        return Err(CaptureError::Worker {
            message: format!(
                "Excel session returned no receipt ({}): {stdout}",
                output.status
            ),
            stderr,
        });
    };
    validate_receipt(&receipt).map_err(|message| CaptureError::Worker {
        message,
        stderr: stderr.clone(),
    })?;
    let digest = |path: &Path| std::fs::read(path).map(|b| calc_verdict::sha256_hex(&b));
    if receipt["script"].as_str() != script.to_str()
        || receipt["environment"] != environment
        || receipt["script_sha256"].as_str() != digest(script).ok().as_deref()
        || receipt["result_sha256"].as_str() != digest(result).ok().as_deref()
    {
        return Err(CaptureError::Worker {
            message: "Excel session receipt is not bound to this worker and raw result".into(),
            stderr,
        });
    }
    if let Some(path) = receipt_path {
        std::fs::copy(path, work.join("session-receipt.json")).map_err(|e| {
            CaptureError::Setup {
                message: format!("cannot retain Excel session receipt: {e}"),
            }
        })?;
    }
    if !output.status.success() {
        return Err(CaptureError::Worker {
            message: format!("Excel session client failed: {stdout}"),
            stderr,
        });
    }
    Ok(WorkerRun {
        worker_pid: receipt["host_pid"].as_u64().unwrap() as u32,
        excel_pid: Some(receipt["excel"]["pid"].as_u64().unwrap() as u32),
        excel: ReportedExcel::SessionReady(receipt_path.unwrap().to_owned()),
        exit_code: Some(0),
        done: true,
        other_output: stdout.lines().map(str::to_owned).collect(),
        stderr,
    })
}

fn validate_receipt(receipt: &Value) -> Result<(), String> {
    let positive_pid = |v: &Value| {
        v.as_u64()
            .is_some_and(|p| p > 0 && p <= u64::from(u32::MAX))
    };
    if receipt["format"] != "dnacalc-excel-session-job/1"
        || receipt["success"] != true
        || !receipt["error"].is_null()
        || !positive_pid(&receipt["host_pid"])
        || !positive_pid(&receipt["excel"]["pid"])
        || receipt["excel"]["pid"] == 3144
        || !receipt["excel"]["start_ticks"].is_u64()
        || !receipt["baseline"].is_object()
        || receipt["baseline_before"] != receipt["baseline"]
        || receipt["baseline_after"] != receipt["baseline"]
    {
        return Err(format!(
            "Excel session job lacks verified ownership and baseline: {receipt}"
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Native-only: validates the native Excel session's JSON receipt contract.
    #[test]
    fn refuses_missing_baseline_drift_and_unowned_processes() {
        let valid = json!({"format":"dnacalc-excel-session-job/1","success":true,"error":null,
            "host_pid":123,"excel":{"pid":456,"start_ticks":789},
            "baseline":{"workbooks":[]},"baseline_before":{"workbooks":[]},"baseline_after":{"workbooks":[]}});
        assert!(validate_receipt(&valid).is_ok());
        for key in [
            "baseline",
            "baseline_before",
            "baseline_after",
            "excel",
            "host_pid",
            "success",
        ] {
            let mut bad = valid.clone();
            bad.as_object_mut().unwrap().remove(key);
            assert!(validate_receipt(&bad).is_err(), "missing {key}");
        }
        let mut drift = valid.clone();
        drift["baseline_after"]["workbooks"] = json!(["leaked.xlsx"]);
        assert!(validate_receipt(&drift).is_err());
        let mut protected = valid;
        protected["excel"]["pid"] = json!(3144);
        assert!(validate_receipt(&protected).is_err());
    }
}
