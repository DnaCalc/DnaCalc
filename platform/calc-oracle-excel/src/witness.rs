//! Bulk witness readings (bead dc-ymjf.3): Excel's result of one worksheet function on many
//! argument rows, read through `Range.Value2` under the isolation every capture runs in
//! ([`run_isolated`]): one worker process ([`WITNESS_SCRIPT`], which reuses the capture worker's
//! functions), one new Excel instance it reports by PID, and termination only of that worker and
//! of an Excel this run started. No workbook is opened; the worker adds an empty one.
//!
//! Arguments are written as typed cell values (a number as the double of its bits), never as
//! formula text, and read back: a row whose arguments Excel changed (`input_kept` false) is a
//! reading of other inputs, and [`WitnessReading::value`] says so instead of giving a value. Each
//! outcome records the environment it was read in: Excel's version and build, the bitness of its
//! executable, the OS and the CPU, the fields a witness reading carries.
//!
//! The work directory a request is given (it must not exist yet, or be empty) receives:
//!
//! ```text
//! <work>/capture-worker.ps1  the capture worker, whose functions the witness worker uses
//! <work>/witness-worker.ps1  the witness worker
//! <work>/request.json        the worker's request
//! <work>/result.json         the worker's result
//! ```

use crate::capture::{excel_available, windows_powershell};
use crate::convert::raw_value;
use crate::isolation::{EXCEL_IMAGE, Worker, WorkerRun, run_isolated};
use crate::{CaptureError, WITNESS_SCRIPT, WORKER_SCRIPT};
use calc_verdict::value::{CalcValue, CoreValue};
use serde_json::{Value, json};
use std::ffi::OsString;
use std::fs;
use std::path::PathBuf;
use std::time::Duration;

/// How long the Excel a witness worker reported may outlive the worker's normal exit before it is
/// terminated.
const EXIT_GRACE: Duration = Duration::from_secs(15);

/// One call to read: its arguments (numbers, logicals and text; an array argument is refused
/// before Excel starts) and, for a function that returns an array, the cell to read (1-based).
#[derive(Clone, Debug)]
pub struct WitnessCall {
    pub id: String,
    pub args: Vec<CalcValue>,
    pub result_index: Option<(usize, usize)>,
}

/// One function's calls to read in one Excel instance.
#[derive(Clone, Debug)]
pub struct WitnessRequest {
    /// The function as a worksheet formula names it in English (`GAMMALN`, `NORM.S.DIST`).
    pub function: String,
    pub calls: Vec<WitnessCall>,
    /// The work directory; it must not exist yet, or be empty.
    pub work_dir: PathBuf,
    /// How long the worker may run before it and its Excel are terminated.
    pub timeout: Duration,
}

/// Excel's reading of one call: the value, or why there is none.
#[derive(Clone, Debug, PartialEq)]
pub struct WitnessReading {
    pub id: String,
    pub value: Result<CalcValue, String>,
}

/// Where the readings were taken.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WitnessEnvironment {
    /// `Application.Version`, for example `16.0`.
    pub excel_version: String,
    /// `Application.Build`, for example `20527`.
    pub excel_build: String,
    pub excel_file_version: String,
    /// `64-bit` or `32-bit`, from the machine type of EXCEL.EXE.
    pub bitness: String,
    pub platform: String,
    pub os_version: String,
    pub cpu: Option<String>,
    pub automation_security_before: i64,
    pub automation_security_after: i64,
}

/// The readings of one request and the run that took them.
#[derive(Clone, Debug)]
pub struct WitnessOutcome {
    pub environment: WitnessEnvironment,
    pub readings: Vec<WitnessReading>,
    pub run: WorkerRun,
}

fn setup(message: String) -> CaptureError {
    CaptureError::Setup { message }
}

/// A call's argument in the worker's request form, or why it has none.
fn request_argument(value: &CalcValue) -> Result<Value, String> {
    match value.core() {
        CoreValue::Number(n) => Ok(json!({"number": format!("{:016x}", n.to_bits())})),
        CoreValue::Logical(b) => Ok(json!({"logical": b})),
        CoreValue::Text(t) => Ok(json!({"text": t.to_string_lossy()})),
        _ => Err("an argument that is not a number, a logical or text".to_owned()),
    }
}

/// The formula of a call with `count` arguments in the columns to its right, in R1C1 notation.
fn formula(function: &str, count: usize, index: Option<(usize, usize)>) -> String {
    let args: Vec<String> = (1..=count).map(|c| format!("RC[{c}]")).collect();
    let call = format!("{function}({})", args.join(","));
    match index {
        Some((r, c)) => format!("=INDEX({call},{r},{c})"),
        None => format!("={call}"),
    }
}

/// Reads Excel's result of every call of `request` (see the module documentation). The worker
/// writes the calls in the order given, a run of rows with one formula at a time; sorting calls
/// by argument count keeps the runs long.
pub fn read_witnesses(request: &WitnessRequest) -> Result<WitnessOutcome, CaptureError> {
    if request.function.is_empty()
        || !request
            .function
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'.' || b == b'_')
    {
        return Err(setup(format!("{:?} is not a function name", request.function)));
    }
    let mut rows = Vec::with_capacity(request.calls.len());
    for call in &request.calls {
        let args = call
            .args
            .iter()
            .map(request_argument)
            .collect::<Result<Vec<_>, _>>()
            .map_err(|e| setup(format!("call {}: {e}", call.id)))?;
        rows.push(json!({
            "id": call.id,
            "args": args,
            "formula": formula(&request.function, call.args.len(), call.result_index),
        }));
    }
    excel_available().map_err(|reason| CaptureError::Unavailable { reason })?;
    let work = std::path::absolute(&request.work_dir)
        .map_err(|e| setup(format!("cannot resolve {}: {e}", request.work_dir.display())))?;
    if work.exists()
        && fs::read_dir(&work)
            .map(|mut entries| entries.next().is_some())
            .unwrap_or(true)
    {
        return Err(setup(format!(
            "the work directory {} is not empty; a witness reading needs a fresh one",
            work.display()
        )));
    }
    fs::create_dir_all(&work).map_err(|e| setup(format!("cannot create {}: {e}", work.display())))?;
    let script = work.join("witness-worker.ps1");
    for (path, text) in [
        (work.join("capture-worker.ps1"), WORKER_SCRIPT),
        (script.clone(), WITNESS_SCRIPT),
    ] {
        fs::write(&path, text).map_err(|e| setup(format!("cannot write {}: {e}", path.display())))?;
    }
    let request_file = work.join("request.json");
    let result_file = work.join("result.json");
    let worker_request = json!({
        "format": "dnacalc-witness-request",
        "version": 1,
        "function": request.function,
        "rows": rows,
    });
    fs::write(&request_file, worker_request.to_string())
        .map_err(|e| setup(format!("cannot write {}: {e}", request_file.display())))?;
    let args: Vec<OsString> = [
        "-NoLogo",
        "-NoProfile",
        "-NonInteractive",
        "-ExecutionPolicy",
        "Bypass",
        "-File",
    ]
    .iter()
    .map(OsString::from)
    .chain([
        script.into_os_string(),
        OsString::from("-Request"),
        request_file.into_os_string(),
        OsString::from("-Result"),
        result_file.clone().into_os_string(),
    ])
    .collect();
    let run = run_isolated(&Worker {
        program: windows_powershell(),
        args,
        dir: work.clone(),
        timeout: request.timeout,
        exit_grace: EXIT_GRACE,
        excel_image: EXCEL_IMAGE.to_owned(),
    })?;
    let result: Option<Value> = fs::read(&result_file)
        .ok()
        .and_then(|bytes| serde_json::from_slice(&bytes).ok());
    match result {
        Some(result) if run.done && run.exit_code == Some(0) => {
            if result["excel_pid"].as_u64() != run.excel_pid.map(u64::from) {
                return Err(CaptureError::Result {
                    message: format!(
                        "the result names Excel PID {} and the worker reported {:?}",
                        result["excel_pid"], run.excel_pid
                    ),
                });
            }
            let (environment, readings) =
                convert_result(&result, request.calls.len()).map_err(|message| CaptureError::Result { message })?;
            if readings.iter().zip(&request.calls).any(|(reading, call)| reading.id != call.id) {
                return Err(CaptureError::Result { message: "the witness result reordered or renamed input rows".into() });
            }
            Ok(WitnessOutcome {
                environment,
                readings,
                run,
            })
        }
        result => Err(CaptureError::Worker {
            message: format!(
                "the witness worker (PID {}) exited with {:?}: {}; {}",
                run.worker_pid,
                run.exit_code,
                result
                    .as_ref()
                    .and_then(|result| result["fatal"].as_str())
                    .unwrap_or("it wrote no result"),
                run.excel.describe(run.excel_pid)
            ),
            stderr: run.stderr.clone(),
        }),
    }
}

/// The worker's result as the environment and one reading per call, in the order asked.
fn convert_result(
    result: &Value,
    calls: usize,
) -> Result<(WitnessEnvironment, Vec<WitnessReading>), String> {
    let text = |name: &str| -> Result<String, String> {
        result[name]
            .as_str()
            .filter(|s| !s.is_empty())
            .map(str::to_owned)
            .ok_or_else(|| format!("the result has no {name}"))
    };
    let bitness = match text("excel_machine")?.as_str() {
        "x86-64" | "arm64" => "64-bit".to_owned(),
        "x86" => "32-bit".to_owned(),
        other => other.to_owned(),
    };
    let security_before = result["automation_security_before"].as_i64().ok_or("missing initial AutomationSecurity reading")?;
    let security_after = result["automation_security_after"].as_i64().ok_or("missing final AutomationSecurity reading")?;
    if !(1..=3).contains(&security_before) { return Err("unknown AutomationSecurity reading".into()); }
    if security_before != security_after { return Err("AutomationSecurity changed during the reading".into()); }
    let environment = WitnessEnvironment {
        excel_version: text("excel_version")?,
        excel_build: text("excel_build")?,
        excel_file_version: text("excel_file_version")?,
        bitness,
        platform: "Windows".to_owned(),
        os_version: text("os_version")?,
        cpu: Some(text("cpu")?),
        automation_security_before: security_before,
        automation_security_after: security_after,
    };
    let rows = result["rows"]
        .as_array()
        .ok_or("the result has no rows")?;
    if rows.len() != calls {
        return Err(format!("the result holds {} rows for {calls} calls", rows.len()));
    }
    let readings = rows
        .iter()
        .map(|row| {
            let id = row["id"].as_str().unwrap_or_default().to_owned();
            let value = if row["input_kept"].as_bool() == Some(true) {
                raw_value(&row["value"])
            } else {
                Err("Excel did not keep the arguments as written".to_owned())
            };
            WitnessReading { id, value }
        })
        .collect();
    Ok((environment, readings))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn formulas_refer_to_the_argument_cells_to_their_right() {
        assert_eq!(formula("GAMMALN", 1, None), "=GAMMALN(RC[1])");
        assert_eq!(formula("NORM.DIST", 4, None), "=NORM.DIST(RC[1],RC[2],RC[3],RC[4])");
        assert_eq!(formula("MINVERSE", 1, Some((1, 2))), "=INDEX(MINVERSE(RC[1]),1,2)");
        assert_eq!(formula("PI", 0, None), "=PI()");
    }

    #[test]
    fn arguments_take_the_request_form_and_arrays_are_refused() {
        assert_eq!(
            request_argument(&CalcValue::number(1.0)).unwrap(),
            json!({"number": "3ff0000000000000"})
        );
        assert_eq!(request_argument(&CalcValue::logical(true)).unwrap(), json!({"logical": true}));
        let array = calc_verdict::decode_argument(&json!([["0x3ff0000000000000"]]), "a").unwrap();
        assert!(request_argument(&array).is_err());
    }

    #[test]
    fn a_result_converts_and_a_changed_input_is_no_reading() {
        let result = json!({
            "excel_version": "16.0", "excel_build": "20527", "excel_file_version": "16.0.20527.20000",
            "automation_security_before": 1, "automation_security_after": 1,
            "excel_machine": "x86-64", "os_version": "Microsoft Windows NT 10.0.26200.0", "cpu": "C",
            "rows": [
                {"id": "a", "value": {"number": "3ff0000000000000"}, "input_kept": true},
                {"id": "b", "value": {"error_code": 2036}, "input_kept": true},
                {"id": "c", "value": {"number": "0000000000000000"}, "input_kept": false}
            ]
        });
        let (env, readings) = convert_result(&result, 3).unwrap();
        assert_eq!((env.excel_build.as_str(), env.bitness.as_str()), ("20527", "64-bit"));
        assert_eq!(readings[0].value, Ok(CalcValue::number(1.0)));
        assert_eq!(
            readings[1].value,
            Ok(CalcValue::error(calc_verdict::value::WorksheetErrorCode::Num))
        );
        assert!(readings[2].value.is_err());
        assert!(convert_result(&result, 2).is_err());
        assert_eq!((env.automation_security_before, env.automation_security_after), (1, 1));
        let mut changed = result.clone();
        changed["automation_security_after"] = json!(3);
        assert!(convert_result(&changed, 3).unwrap_err().contains("AutomationSecurity changed"));
        changed["automation_security_before"] = Value::Null;
        assert!(convert_result(&changed, 3).is_err());
        assert!(!crate::WITNESS_SCRIPT.contains(".AutomationSecurity ="));
    }
}
