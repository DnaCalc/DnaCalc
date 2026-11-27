//! Build a fresh workbook for an authored Touchstone VBA case and use the socket client to ask it.
use calc_doc::model::value::WorksheetErrorCode;
use calc_doc::{SourceModule, SourceModuleKind, source_vba_package_for_sheets};
use calc_verdict::{ReadingSource, WitnessInvocation, WitnessRow, WitnessSet};
use serde::Deserialize;
use serde_json::{Value, json};
use std::fs::{self, File};
use std::io::{self, Read, Write};
use std::path::Path;
use std::process::{Command, ExitCode};
use xtask::cargo::Workspace;

pub const USAGE: &str = concat!(
    "usage: cargo xtask vba-case admit [<catalogue-dir>] | build [<case.json>] | prepare <case.json> | ",
    "capture <case.json> --out <dir> [--timeout <seconds>] | ",
    "witness <capture.json> <prediction.json> --out <bank-dir>",
);

#[derive(Deserialize)]
struct Layout {
    sheets: Vec<Sheet>,
    modules: Vec<Module>,
}

#[derive(Deserialize)]
struct Sheet {
    name: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "lowercase")]
enum ModuleKind {
    Standard,
    Document,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Module {
    name: String,
    kind: ModuleKind,
    source: String,
}

fn build(input: impl Read) -> Result<Vec<u8>, String> {
    const MAX_INPUT: u64 = 64 * 1024;
    let mut json = Vec::new();
    input
        .take(MAX_INPUT + 1)
        .read_to_end(&mut json)
        .map_err(|error| format!("reading the case: {error}"))?;
    if json.len() as u64 > MAX_INPUT {
        return Err("VBA case exceeds 64 KiB".into());
    }

    let layout: Layout = serde_json::from_slice(&json).map_err(|error| error.to_string())?;
    let sheets: Vec<_> = layout
        .sheets
        .iter()
        .map(|sheet| sheet.name.as_str())
        .collect();
    let modules: Vec<_> = layout
        .modules
        .iter()
        .map(|module| SourceModule {
            name: &module.name,
            source: &module.source,
            kind: match module.kind {
                ModuleKind::Standard => SourceModuleKind::Standard,
                ModuleKind::Document if module.name == "ThisWorkbook" => SourceModuleKind::Workbook,
                ModuleKind::Document => SourceModuleKind::Worksheet,
            },
        })
        .collect();
    source_vba_package_for_sheets(&sheets, &modules).map_err(|error| error.to_string())
}

fn write_workbook(input: impl Read) -> Result<ExitCode, String> {
    io::stdout()
        .lock()
        .write_all(&build(input)?)
        .map_err(|error| format!("writing the .xlsm: {error}"))?;
    Ok(ExitCode::SUCCESS)
}

fn run_client(args: &[String]) -> Result<ExitCode, String> {
    let cwd = std::env::current_dir().map_err(|error| error.to_string())?;
    let root = Workspace::load(&cwd)?.root;
    let python = if cfg!(windows) { "python" } else { "python3" };
    let harness = std::env::current_exe().map_err(|error| error.to_string())?;
    let status = Command::new(python)
        .arg(root.join("tools/excel-service/touchstone/client.py"))
        .args(args)
        .env("DNACALC_XTASK_HARNESS", harness)
        .status()
        .map_err(|error| format!("starting the Touchstone client: {error}"))?;
    Ok(ExitCode::from(
        status
            .code()
            .and_then(|code| u8::try_from(code).ok())
            .unwrap_or(1),
    ))
}

fn bank_argument(arg: &Value) -> Result<Value, String> {
    match arg["type"].as_str() {
        Some("number") => arg["bits"]
            .as_str()
            .filter(|bits| bits.len() == 16 && bits.bytes().all(|byte| byte.is_ascii_hexdigit()))
            .map(|bits| json!(format!("0x{bits}"))),
        Some("text") => arg["value"].as_str().map(|value| json!(value)),
        Some("boolean") => arg["value"].as_bool().map(|value| json!(value)),
        Some("blank") => Some(Value::Null),
        Some("error") => WorksheetErrorCode::all()
            .into_iter()
            .find(|code| arg["code"] == code.excel_spelling())
            .map(|code| json!({"error":format!("{code:?}")})),
        _ => None,
    }
    .ok_or_else(|| "capture contains an invalid VBA argument".to_owned())
}

fn case_witness(
    capture: &Value,
    prediction: &Value,
    source: &Path,
) -> Result<Option<WitnessSet>, String> {
    let reply = &capture["reply"];
    let actual = &reply["rows"][0];
    let build = reply["excel_build"]
        .as_str()
        .filter(|build| !build.is_empty());
    if reply["engine"] != "excel"
        || reply["ok"] != true
        || !is_vba_result(actual)
        || build.is_none()
        || has_unread_value(actual)
    {
        return Err("expected a complete VBA capture with Excel build provenance".into());
    }
    if !is_vba_result(prediction) {
        return Err(
            "expected a frozen typed VBA result with calls, sheet_state and event_log".into(),
        );
    }
    if actual == prediction {
        return Ok(None);
    }

    let source_hash = capture_hex(&reply["source_sha256"], 64)?;
    let id = capture_hex(&reply["id"], 32)?;
    let timestamp = reply["timestamp"]
        .as_str()
        .ok_or("missing capture timestamp")?;
    let lane = reply["lane"].as_str().unwrap_or("unknown lane");
    let calls = capture["case"]["calls"]
        .as_array()
        .ok_or("missing VBA calls")?;
    let mut args = Vec::with_capacity(calls.len());
    let mut procedures = Vec::with_capacity(calls.len());
    for call in calls {
        let procedure = call["procedure"].as_str().ok_or("missing VBA procedure")?;
        procedures.push(procedure);
        let mut invocation = vec![json!(procedure)];
        for arg in call["args"]
            .as_array()
            .ok_or("missing VBA call arguments")?
        {
            invocation.push(bank_argument(arg)?);
        }
        args.push(Value::Array(invocation));
    }

    // One case shares workbook and module state across its calls. The WitnessSet text
    // encoding retains that whole typed observation, including Err and ordered events.
    Ok(Some(WitnessSet {
        function: procedures.join(", "),
        selection: format!("VBA case with source SHA-256 {source_hash}"),
        readings: vec![ReadingSource {
            id: "excel".into(),
            source: format!("{} ({lane}, {timestamp})", source.display()),
            excel_build: build.map(str::to_owned),
            platform: Some("Windows".into()),
            bitness: None,
            cpu: reply["cpu"].as_str().map(str::to_owned),
            invocation: Some(WitnessInvocation {
                input_transport: Some("touchstone-vba".into()),
                ..Default::default()
            }),
        }],
        witnesses: vec![WitnessRow {
            id: id.into(),
            args,
            result_index: None,
            expected: format!("text:{actual}"),
            reading: "excel".into(),
            calc_func: Some(format!("text:{prediction}")),
        }],
    }))
}

fn is_vba_result(value: &Value) -> bool {
    value["type"] == "vba_case"
        && ["calls", "sheet_state", "event_log"]
            .iter()
            .all(|key| value[key].is_array())
}

fn has_unread_value(value: &Value) -> bool {
    match value {
        Value::Array(items) => items.iter().any(has_unread_value),
        Value::Object(fields) => value["type"] == "unread" || fields.values().any(has_unread_value),
        _ => false,
    }
}

fn capture_hex(value: &Value, length: usize) -> Result<&str, String> {
    value
        .as_str()
        .filter(|hex| hex.len() == length && hex.bytes().all(|byte| byte.is_ascii_hexdigit()))
        .ok_or_else(|| "invalid VBA capture hash or request ID".to_owned())
}

fn read_json(path: &Path) -> Result<Value, String> {
    let bytes = fs::read(path).map_err(|error| format!("reading {}: {error}", path.display()))?;
    serde_json::from_slice(&bytes).map_err(|error| format!("parsing {}: {error}", path.display()))
}

fn write_witness(source: &str, prediction: &str, out: &str) -> Result<ExitCode, String> {
    let source = Path::new(source);
    let capture = read_json(source)?;
    let prediction = read_json(Path::new(prediction))?;
    let Some(set) = case_witness(&capture, &prediction, source)? else {
        println!("Excel matched the frozen VBA prediction");
        return Ok(ExitCode::SUCCESS);
    };
    let hash = capture_hex(&capture["reply"]["source_sha256"], 64)?;
    let id = &set.witnesses[0].id;
    let out = Path::new(out);
    fs::create_dir_all(out).map_err(|error| format!("creating {}: {error}", out.display()))?;
    let path = out.join(format!("vba-{hash}-{id}.json"));
    let encoded = set.encode();
    match File::create_new(&path) {
        Ok(mut file) => {
            if let Err(error) = file.write_all(encoded.as_bytes()) {
                drop(file);
                let _ = fs::remove_file(&path);
                return Err(format!("writing {}: {error}", path.display()));
            }
        }
        Err(error) if error.kind() == io::ErrorKind::AlreadyExists => {
            if fs::read(&path).as_deref().ok() != Some(encoded.as_bytes()) {
                return Err(format!(
                    "{} already contains a different witness",
                    path.display()
                ));
            }
        }
        Err(error) => return Err(format!("creating {}: {error}", path.display())),
    }
    println!("{}", path.display());
    Ok(ExitCode::SUCCESS)
}

pub fn run(args: &[String]) -> ExitCode {
    let result = match args {
        [command] if command == "build" => write_workbook(io::stdin().lock()),
        [command, path] if command == "build" => File::open(path)
            .map_err(|error| format!("opening {path}: {error}"))
            .and_then(write_workbook),
        [command, ..] if matches!(command.as_str(), "admit" | "prepare" | "capture") => run_client(args),
        [command, capture, prediction, out_flag, out]
            if command == "witness" && out_flag == "--out" =>
        {
            write_witness(capture, prediction, out)
        }
        _ => {
            eprintln!("{USAGE}");
            return ExitCode::from(2);
        }
    };
    result.unwrap_or_else(|error| {
        eprintln!("xtask vba-case: {error}");
        ExitCode::FAILURE
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;

    #[test]
    fn case_builds_a_deterministic_workbook_with_ordered_tab_code_names() {
        let case = br#"{
            "sheets": [{"name":"Data & results"}, {"name":"Events"}],
            "modules": [
                {"name":"Module1","kind":"standard","source":"Function Probe() As Variant\nEnd Function"},
                {"name":"Sheet2","kind":"document","source":"Option Explicit"}
            ]
        }"#;
        let bytes = build(case.as_slice()).unwrap();
        assert_eq!(build(case.as_slice()).unwrap(), bytes);
        let doc = calc_doc::open(
            Arc::new(calc_doc::MemorySource::new(bytes)),
            &Default::default(),
        )
        .unwrap();
        let book = calc_doc::load(&doc, &mut Default::default()).unwrap();
        assert_eq!(book.spreadsheet_format(), calc_doc::SpreadsheetFormat::Xlsm);
        assert_eq!(
            book.model()
                .sheets()
                .iter()
                .map(|sheet| sheet.name().text().to_string_lossy())
                .collect::<Vec<_>>(),
            ["Data & results", "Events"],
        );
        assert_eq!(
            book.vba_document_names().unwrap(),
            [
                ("ThisWorkbook".into(), None),
                ("Sheet1".into(), Some(book.sheets()[0].id())),
                ("Sheet2".into(), Some(book.sheets()[1].id())),
            ]
        );
    }

    #[test]
    fn case_refuses_a_document_module_without_a_matching_tab() {
        let case = br#"{
            "sheets": [{"name":"Data"}],
            "modules": [{"name":"Sheet2","kind":"document","source":"Option Explicit"}]
        }"#;
        assert!(build(case.as_slice()).unwrap_err().contains("Sheet2"));
    }

    #[test]
    fn witness_keeps_only_vba_differences_and_preserves_bits_and_events() {
        let result = json!({
            "type": "vba_case",
            "calls": [{"return": {"type": "number", "bits": "4000000000000000"}}],
            "sheet_state": [],
            "event_log": [{"at": "Data!B1", "value": {"type": "text", "value": "Change"}}],
        });
        let capture = json!({
            "case": {"calls": [{"procedure": "Module1.Probe", "args": [
                {"type": "number", "bits": "3ff0000000000000"}
            ]}]},
            "reply": {
                "engine": "excel", "ok": true, "excel_build": "20625", "cpu": "test CPU",
                "lane": "vb-test", "timestamp": "2026-11-25T22:00:00Z",
                "source_sha256": "a".repeat(64), "id": "1".repeat(32), "rows": [result],
            },
        });
        let source = Path::new("capture.json");
        assert!(case_witness(&capture, &result, source).unwrap().is_none());
        let mut predicted = result.clone();
        predicted["event_log"] = json!([]);
        let set = case_witness(&capture, &predicted, source).unwrap().unwrap();
        assert_eq!(WitnessSet::decode(set.encode().as_bytes()).unwrap(), set);
        assert_eq!(set.function, "Module1.Probe");
        assert!(set.selection.ends_with(&"a".repeat(64)));
        assert_eq!(
            set.witnesses[0].args,
            [json!(["Module1.Probe", "0x3ff0000000000000"])]
        );
        assert_eq!(set.witnesses[0].expected, format!("text:{result}"));
        assert_eq!(set.readings[0].excel_build.as_deref(), Some("20625"));
        assert_eq!(set.readings[0].cpu.as_deref(), Some("test CPU"));
        assert!(
            set.readings[0]
                .source
                .contains("vb-test, 2026-11-25T22:00:00Z")
        );
        let mut fake = capture;
        fake["reply"]["engine"] = json!("fake");
        assert!(case_witness(&fake, &predicted, source).is_err());
    }
}
