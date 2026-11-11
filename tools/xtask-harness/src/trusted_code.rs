//! Explicit execution policy for our trusted evidence packages. This extends compare's
//! existing file-project and native-provider composition; it supplies no Excel answers.
use crate::compare_triage::{Class, WorkbookFailure};
use calc_fml::CallableTable;
use calc_host::{HostSession, HostState};
use calc_verdict::{CaptureRecord, sha256_hex};
use serde_json::{Value, json};
use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
    rc::Rc,
};

#[derive(Clone, Debug)]
struct Artifact {
    path: PathBuf,
    digest: String,
}

#[derive(Clone, Debug)]
struct Entry {
    vba: bool,
    events: bool,
    xlls: Vec<Artifact>,
    live_capture: Option<String>,
}

#[derive(Debug)]
pub(crate) struct Manifest {
    pub(crate) digest: String,
    entries: BTreeMap<String, Entry>,
    /// `DNACALC_TRUST_ALL_MACROS=1` (dc-15dk): every workbook file's project and events run unless
    /// the capture's Excel ran with macros disabled, in addition to the entries above.
    trust_all: Option<&'static str>,
}

fn digest(text: &str) -> bool {
    text.len() == 64
        && text
            .bytes()
            .all(|b| b.is_ascii_hexdigit() && !b.is_ascii_uppercase())
}

fn failure(class: Class, key: &str, message: impl Into<String>) -> WorkbookFailure {
    WorkbookFailure {
        class,
        key: format!("{key}:dc-ztvv.13"),
        message: message.into(),
    }
}

impl Manifest {
    pub(crate) fn read(path: &Path, expected: Option<&str>) -> Result<Self, String> {
        let bytes = std::fs::read(path).map_err(|e| format!("trusted-code manifest: {e}"))?;
        let actual = sha256_hex(&bytes);
        if expected.is_some_and(|expected| expected != actual) {
            return Err("trusted-code manifest changed after corpus admission".into());
        }
        let value: Value = serde_json::from_slice(&bytes).map_err(|e| e.to_string())?;
        if value["format"] != "dnacalc-trusted-evidence-code" || value["version"] != 1 {
            return Err("invalid trusted-evidence code manifest".into());
        }
        let mut entries = BTreeMap::new();
        for (hash, entry) in value["workbooks"]
            .as_object()
            .ok_or("manifest needs workbooks")?
        {
            if !digest(hash)
                || entry["evidence"]
                    .as_str()
                    .is_none_or(|s| s.trim().is_empty())
            {
                return Err("trusted workbook requires its exact hash and evidence".into());
            }
            let vba = entry["vba"]
                .as_bool()
                .ok_or("trusted workbook needs explicit vba policy")?;
            let events = entry["events"]
                .as_bool()
                .ok_or("trusted workbook needs explicit events policy")?;
            if events && !vba {
                return Err("events require an enabled file project".into());
            }
            let mut xlls = Vec::new();
            for artifact in entry["xlls"]
                .as_array()
                .ok_or("trusted workbook needs an xlls list")?
            {
                let path = PathBuf::from(artifact["path"].as_str().ok_or("XLL needs a path")?);
                if !path.is_absolute() {
                    return Err(
                        "XLL artifact paths must be absolute so the manifest binds their origin"
                            .into(),
                    );
                }
                let hash = artifact["sha256"]
                    .as_str()
                    .filter(|s| digest(s))
                    .ok_or("XLL needs an exact hash")?;
                xlls.push(Artifact {
                    path,
                    digest: hash.into(),
                });
            }
            let live_capture = entry["live_capture_sha256"].as_str().map(str::to_owned);
            if live_capture.as_deref().is_some_and(|s| !digest(s)) {
                return Err("live_capture_sha256 needs an exact capture hash".into());
            }
            entries.insert(
                hash.clone(),
                Entry {
                    vba,
                    events,
                    xlls,
                    live_capture,
                },
            );
        }
        Ok(Self {
            digest: actual,
            entries,
            trust_all: None,
        })
    }

    /// The route for scripts, agents and the corpus harness: the environment variable
    /// `DNACALC_TRUST_ALL_MACROS=1` (named by `by`) trusts every workbook file's project, with no
    /// manifest entry; a capture whose Excel ran macros disabled still compares with the project
    /// held, since running it here would not be the same experiment.
    pub(crate) fn trust_all(by: &'static str) -> Self {
        Self {
            digest: format!("trust-all:{by}"),
            entries: BTreeMap::new(),
            trust_all: Some(by),
        }
    }

    pub(crate) fn policy(&self, package: &str) -> Value {
        if let (None, Some(by)) = (self.entries.get(package), self.trust_all) {
            return json!({"route":"trust-all-macros","trusted_by":by,"manifest_sha256":null,
                "authorized_package":true,"vba":true,"events":true,"live_capture_sha256":null,"xlls":[]});
        }
        let entry = self.entries.get(package);
        json!({"route":"trusted-evidence-code","manifest_sha256":self.digest,
            "authorized_package":entry.is_some(),"vba":entry.is_some_and(|e|e.vba),
            "events":entry.is_some_and(|e|e.events),
            "live_capture_sha256":entry.and_then(|e|e.live_capture.as_deref()),
            "xlls":entry.map(|e|e.xlls.iter().map(|a|json!({"path":a.path,"sha256":a.digest})).collect::<Vec<_>>()).unwrap_or_default()})
    }

    pub(crate) fn prepare(
        &self,
        host: &HostState,
        name: &str,
        capture: &CaptureRecord,
        package: &str,
        capture_digest: Option<&str>,
        linked: bool,
    ) -> Result<PreparedCode, WorkbookFailure> {
        let listed = self.entries.get(package);
        // The trust-all route: no entry, so the capture decides (a DNA-only run has no capture
        // and runs the project with its events).
        let trusted_by_all = match (listed, self.trust_all) {
            (None, Some(by)) => {
                let setting = |key: &str| crate::compare::capture_setting(capture, key);
                let observed = !capture.observations.is_empty();
                Some((
                    by,
                    Entry {
                        vba: !observed || setting("automation_security") == Some("1"),
                        events: !observed || setting("enable_events") != Some("false"),
                        xlls: Vec::new(),
                        live_capture: None,
                    },
                ))
            }
            _ => None,
        };
        let entry = listed.or(trusted_by_all.as_ref().map(|(_, entry)| entry));
        if let Some(entry) = entry.filter(|e| !e.xlls.is_empty()) {
            if linked {
                return Err(failure(
                    Class::Context,
                    "XllProvider",
                    "linked native providers require a shared application owner; not modelled",
                ));
            }
            if !capture.observations.is_empty()
                && (capture_digest.is_none() || entry.live_capture.as_deref() != capture_digest)
            {
                return Err(failure(
                    Class::Context,
                    "TrustedCodePolicy",
                    "native live comparison requires an attested exact enabled-provider capture",
                ));
            }
        }
        let vba = entry.is_some_and(|e| e.vba);
        let events = entry.is_some_and(|e| e.events);
        // A disabled or unknown Excel policy is never silently reinterpreted as an
        // enabled capture. DNA-only inventories have no Excel observations at all.
        if !capture.observations.is_empty() && host.loaded().has_vba_project() {
            let setting = |key: &str| {
                capture
                    .environment
                    .as_ref()?
                    .settings
                    .iter()
                    .find(|(k, _)| k == key)
                    .map(|(_, v)| v.as_str())
            };
            let security = if vba { "1" } else { "3" };
            let enabled = if events { "true" } else { "false" };
            if setting("automation_security") != Some(security)
                || (vba && setting("enable_events") != Some(enabled))
            {
                return Err(failure(
                    Class::Context,
                    "TrustedCodePolicy",
                    "live capture and requested trusted code/event policies differ",
                ));
            }
        }
        // The grants the harness makes (dc-0f1k.22): the manifest lists this package's exact
        // bytes (`package` is their SHA-256) with its VBA project enabled, or the environment
        // variable trusts every workbook (dc-15dk).
        let project = if vba {
            let grant = match &trusted_by_all {
                Some((by, _)) => calc_host_vba::ProjectGrant::trust_all(by, calc_macro_security::VbaPolicy::Full),
                None => calc_host_vba::ProjectGrant::evidence_manifest(&self.digest, package),
            };
            calc_host_vba::prepare_file_project_with_events(&grant, host, name, 100_000, None, events)
                .map_err(|e| WorkbookFailure {
                    class: Class::Doc,
                    key: "VbaProject:dc-snz.15".into(),
                    message: e.to_string(),
                })?
        } else {
            None
        };
        let mut functions = project.as_ref().map(|p| p.functions()).unwrap_or_default();
        let native = NativeCode::load(
            host,
            name,
            entry.map(|e| e.xlls.as_slice()).unwrap_or_default(),
        )?;
        let extra = native.functions()?;
        for (provider, name) in extra.registrations() {
            let handle = extra.bind(provider, name).expect("enumerated registration");
            functions
                .register_for(
                    provider,
                    name,
                    Rc::clone(extra.resolve(handle).expect("live registration")),
                )
                .map_err(|e| failure(Class::Context, "CallableCollision", e))?;
        }
        Ok(PreparedCode {
            project,
            functions,
            events,
            native,
        })
    }
}

pub(crate) struct PreparedCode {
    project: Option<Rc<calc_host_vba::WorkbookEvents>>,
    functions: CallableTable,
    events: bool,
    // Drop after the composed registry, and retain this owner longer than its HostSession.
    native: NativeCode,
}

impl PreparedCode {
    pub(crate) fn install(&self, session: &mut HostSession) {
        session.set_workbook_events_enabled(self.events);
        if let Some(project) = &self.project {
            Rc::clone(project).install_with_registry(session, self.functions.clone());
            session.notify_workbook_open();
        } else if self.functions.registrations().next().is_some() {
            session.set_user_functions(self.functions.clone());
        }
    }
    pub(crate) fn poll(
        &self,
        domain: &mut calc_host::CalculationDomain,
        id: calc_host::DomainWorkbookId,
    ) -> Result<bool, String> {
        self.native.poll(domain, id)
    }
    pub(crate) fn has_async(
        &self,
        domain: &calc_host::CalculationDomain,
        id: calc_host::DomainWorkbookId,
    ) -> bool {
        self.native.enabled()
            && domain
                .host(id)
                .is_some_and(|host| !host.pending_async().is_empty())
    }
}

#[derive(Default)]
struct NativeCode {
    #[cfg(windows)]
    host: Option<calc_xll_host::XllHost>,
    #[cfg(windows)]
    leases: Vec<std::fs::File>,
}

impl NativeCode {
    fn load(host: &HostState, name: &str, artifacts: &[Artifact]) -> Result<Self, WorkbookFailure> {
        if artifacts.is_empty() {
            return Ok(Self::default());
        }
        #[cfg(windows)]
        let mut leases = Vec::new();
        for artifact in artifacts {
            #[cfg(windows)]
            let bytes = {
                use std::io::Read;
                use std::os::windows::fs::OpenOptionsExt;
                let mut file = std::fs::OpenOptions::new()
                    .read(true)
                    .share_mode(windows_sys::Win32::Storage::FileSystem::FILE_SHARE_READ)
                    .open(&artifact.path)
                    .map_err(|e| failure(Class::Context, "XllProvider", e.to_string()))?;
                let mut bytes = Vec::new();
                file.read_to_end(&mut bytes)
                    .map_err(|e| failure(Class::Context, "XllProvider", e.to_string()))?;
                leases.push(file);
                bytes
            };
            #[cfg(not(windows))]
            let bytes = std::fs::read(&artifact.path).map_err(|e| {
                failure(
                    Class::Context,
                    "XllProvider",
                    format!("{}: {e}", artifact.path.display()),
                )
            })?;
            if sha256_hex(&bytes) != artifact.digest {
                return Err(failure(
                    Class::Context,
                    "XllProvider",
                    "XLL bytes differ from the trusted manifest",
                ));
            }
        }
        #[cfg(not(windows))]
        {
            let _ = (host, name);
            Err(failure(
                Class::Context,
                "XllProvider",
                "native XLL providers require Windows x64; no provider code ran",
            ))
        }
        #[cfg(windows)]
        {
            let owner = Self {
                host: Some(calc_xll_host::XllHost::new(0)),
                leases,
            };
            let native = owner.host.as_ref().expect("created native owner");
            native
                .set_async_continuations(true)
                .map_err(|e| failure(Class::Context, "XllProvider", e))?;
            for (index, sheet) in host.sheets().iter().enumerate() {
                native.set_sheet(
                    sheet.id(),
                    format!("[{name}]{}", sheet.name().text().to_string_lossy()),
                    index == 0,
                );
            }
            for artifact in artifacts {
                native
                    .load(&artifact.path)
                    .map_err(|e| failure(Class::Context, "XllProvider", e))?;
            }
            Ok(owner)
        }
    }
    fn functions(&self) -> Result<CallableTable, WorkbookFailure> {
        #[cfg(windows)]
        if let Some(host) = &self.host {
            return host
                .functions_for_workbook()
                .map_err(|e| failure(Class::Context, "XllProvider", e));
        }
        Ok(CallableTable::new())
    }
    fn enabled(&self) -> bool {
        #[cfg(windows)]
        {
            self.host.is_some()
        }
        #[cfg(not(windows))]
        {
            false
        }
    }
    fn poll(
        &self,
        domain: &mut calc_host::CalculationDomain,
        id: calc_host::DomainWorkbookId,
    ) -> Result<bool, String> {
        #[cfg(windows)]
        if let Some(host) = &self.host {
            let mut received = false;
            for returned in host.take_async_returns() {
                let token = domain
                    .host(id)
                    .ok_or("native return owner closed")?
                    .pending_async()
                    .into_iter()
                    .find(|token| token.request == returned.request)
                    .ok_or("native return has no current owner token")?;
                let disposition = domain
                    .complete_async(id, &token, returned.value?)
                    .map_err(|e| e.to_string())?;
                if disposition != calc_host::AsyncDisposition::Accepted {
                    return Err(format!("native return rejected: {disposition:?}"));
                }
                received = true;
            }
            return Ok(received);
        }
        let _ = (domain, id);
        Ok(false)
    }
}

#[cfg(windows)]
impl Drop for NativeCode {
    fn drop(&mut self) {
        if let Some(host) = self.host.take() {
            if let Err(error) = host.close() {
                eprintln!("trusted native provider shutdown: {error}");
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn host(bytes: &[u8]) -> HostState {
        HostState::open(
            std::sync::Arc::new(calc_host::MemorySource::new(bytes.to_vec())),
            &calc_host::OpenPolicy::default(),
            calc_host::HostOptions::default(),
        )
        .unwrap()
    }
    #[test]
    fn manifest_location_cannot_change_a_relative_provider_origin() {
        let directory = std::env::temp_dir().join(format!("trusted-origin-{}", std::process::id()));
        std::fs::create_dir_all(&directory).unwrap();
        let path = directory.join("manifest.json");
        let mut value = json!({"format":"dnacalc-trusted-evidence-code","version":1,
            "workbooks":{"0".repeat(64):{"vba":false,"events":false,"evidence":"authored origin control",
                "xlls":[{"path":"provider.xll","sha256":"0".repeat(64)}]}}});
        std::fs::write(&path, serde_json::to_vec(&value).unwrap()).unwrap();
        assert!(
            Manifest::read(&path, None)
                .unwrap_err()
                .contains("must be absolute")
        );
        value["workbooks"]["0".repeat(64)]["xlls"][0]["path"] =
            json!(directory.join("provider.xll"));
        std::fs::write(&path, serde_json::to_vec(&value).unwrap()).unwrap();
        let before = Manifest::read(&path, None).unwrap();
        let relocated = directory.join("elsewhere");
        std::fs::create_dir_all(&relocated).unwrap();
        let moved = relocated.join("manifest.json");
        std::fs::copy(&path, &moved).unwrap();
        let after = Manifest::read(&moved, None).unwrap();
        assert_eq!(before.digest, after.digest);
        assert_eq!(
            before.policy(&"0".repeat(64)),
            after.policy(&"0".repeat(64))
        );
    }

    #[test]
    fn trusted_project_admits_complete_class_source_and_keeps_event_refusals_typed() {
        let bytes = include_bytes!(
            "../../../crates/calc-engine/tests/fixtures/excel-calc-probes/P08/observer-cold-off-01.xlsm"
        );
        let hash = sha256_hex(bytes);
        let capture =
            crate::compare_inventory::source_record(bytes, "observer-cold-off-01.xlsm").unwrap();
        let host = host(bytes);
        let mut manifest = Manifest {
            digest: "test".into(),
            entries: BTreeMap::new(),
            trust_all: None,
        };
        // Unlisted linked packages do not acquire the root package's authorization.
        let disabled = manifest
            .prepare(&host, "linked.xlsm", &capture, &hash, None, true)
            .unwrap();
        assert!(disabled.project.is_none());
        manifest.entries.insert(
            hash.clone(),
            Entry {
                vba: true,
                events: false,
                xlls: vec![],
                live_capture: None,
            },
        );
        let original = host
            .loaded()
            .vba_project(&Default::default())
            .unwrap()
            .unwrap();
        let admitted = manifest
            .prepare(&host, "probe.xlsm", &capture, &hash, None, false)
            .unwrap();
        assert!(admitted.project.is_some());
        assert!(admitted.functions.contains("PlainProbe"));
        assert!(admitted.functions.contains("OracleProbe.PlainProbe"));
        assert!(!admitted.functions.contains("ProbeEvents.XL_AfterCalculate"));
        let mut session = HostSession::new(host);
        admitted.install(&mut session);
        assert!(!session.workbook_events_enabled());
        assert!(session.take_workbook_event_failures().failures.is_empty());
        assert_eq!(
            original,
            session
                .host()
                .loaded()
                .vba_project(&Default::default())
                .unwrap()
                .unwrap()
        );
        manifest.entries.get_mut(&hash).unwrap().events = true;
        let error = manifest
            .prepare(session.host(), "probe.xlsm", &capture, &hash, None, false)
            .err()
            .expect("unsupported automatic events still refuse before installation");
        assert_eq!(error.key, "VbaProject:dc-snz.15");
        assert!(
            error.message.contains("unsupported automatic-event prefix"),
            "{error:?}"
        );
        assert_eq!(
            original,
            session
                .host()
                .loaded()
                .vba_project(&Default::default())
                .unwrap()
                .unwrap()
        );
    }
    #[test]
    fn native_capture_attestation_and_linked_owner_are_checked_before_loading() {
        let bytes = calc_host::blank_package();
        let hash = sha256_hex(&bytes);
        let host = host(&bytes);
        let mut capture = crate::compare_inventory::source_record(&bytes, "blank.xlsx").unwrap();
        let manifest = Manifest {
            digest: "test".into(),
            entries: BTreeMap::from([(
                hash.clone(),
                Entry {
                    vba: false,
                    events: false,
                    live_capture: None,
                    xlls: vec![Artifact {
                        path: "absent.xll".into(),
                        digest: "0".repeat(64),
                    }],
                },
            )]),
            trust_all: None,
        };
        let linked = manifest
            .prepare(&host, "blank.xlsx", &capture, &hash, None, true)
            .err()
            .unwrap();
        assert!(linked.message.contains("shared application owner"));
        // Synthetic guard input only; never written as an Excel capture.
        capture.observations.push(calc_verdict::CellObservation {
            cell: calc_verdict::CellAddress::new("Sheet1", 1, 1).unwrap(),
            observation: calc_verdict::Observation::ExcelRaw(calc_host::value::CalcValue::number(
                1.,
            )),
        });
        let live = manifest
            .prepare(
                &host,
                "blank.xlsx",
                &capture,
                &hash,
                Some("unattested"),
                false,
            )
            .err()
            .unwrap();
        assert_eq!(live.key, "TrustedCodePolicy:dc-ztvv.13");
    }
    #[cfg(not(windows))]
    #[test]
    fn exact_native_artifact_has_a_typed_platform_refusal_on_linux() {
        let bytes = calc_host::blank_package();
        let host = host(&bytes);
        let path = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../crates/calc-engine/tests/fixtures/excel-calc-probes/P08/native-order.xll");
        let mut artifact = Artifact {
            digest: sha256_hex(&std::fs::read(&path).unwrap()),
            path,
        };
        let error = NativeCode::load(&host, "blank.xlsx", std::slice::from_ref(&artifact))
            .err()
            .unwrap();
        assert_eq!(error.key, "XllProvider:dc-ztvv.13");
        assert!(error.message.contains("require Windows x64"));
        artifact.digest = "0".repeat(64);
        assert!(
            NativeCode::load(&host, "blank.xlsx", &[artifact])
                .err()
                .unwrap()
                .message
                .contains("bytes differ")
        );
    }
}
