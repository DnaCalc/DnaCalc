//! Optional Excel-saved reference. It uses the ordinary judge and the same
//! calculated results, and never changes the live-capture verdict or denominator.
use calc_doc::XmlEvent;
use calc_host::value::CellKey;
use calc_host::{HostState, MemorySource, OpenPolicy};
use calc_verdict::{
    Acquisition, CacheCellType, CaptureRecord, CellObservation, Observation, SavedCache,
};
use serde_json::{Value, json};
use std::{
    collections::{BTreeMap, BTreeSet, VecDeque},
    sync::Arc,
};

pub(crate) fn unavailable(capture: &CaptureRecord, reason: &str) -> (CaptureRecord, Value) {
    let mut record = capture.clone();
    record.acquisition = Acquisition::FileCacheReplay;
    record.observations.clear();
    (record, json!({"unavailable":reason}))
}

/// Provenance is an explicit, hash-bound assertion by the caller, supported by
/// retained save evidence. Excel metadata alone is insufficient: DNA saves and
/// deliberately patched probe inputs can retain that metadata.
pub(crate) fn reference(
    bytes: &[u8],
    host: &HostState,
    capture: &CaptureRecord,
    origin: Option<&Value>,
) -> Result<(CaptureRecord, Value), String> {
    let mut record = capture.clone();
    record.acquisition = Acquisition::FileCacheReplay;
    record.observations.clear();
    if let Some(session) = &mut record.session {
        session.operations = vec!["read Excel-saved values; no Excel execution".into()];
        session.saved.clear();
        session.reopened.clear();
    }
    let mut info = json!({"rule":"supplemental Excel-saved oracle, only without a live answer; exact package provenance and Excel metadata required; volatile formulas and their readers excluded; saved decimals are not live binary64 observations",
        "source":capture.source,"origin":origin,"excluded":{}});
    let Some(origin) = origin else {
        info["unavailable"] = json!("no attested Excel-save lineage for these package bytes");
        return Ok((record, info));
    };
    let document = calc_doc::open(
        Arc::new(MemorySource::new(bytes.to_vec())),
        &OpenPolicy::default(),
    )
    .map_err(|e| e.to_string())?;
    let package = document.package();
    let app = package
        .read_xml("docProps/app.xml")
        .map_err(|e| e.to_string())?;
    let mut application = String::new();
    let mut version = String::new();
    let mut field = String::new();
    let mut reader = app.reader();
    while let Some(event) = reader.next_event().map_err(|e| e.to_string())? {
        match event {
            XmlEvent::Start(e) => field = e.name.local.to_string(),
            XmlEvent::Text(t) if field == "Application" => application.push_str(&t),
            XmlEvent::Text(t) if field == "AppVersion" => version.push_str(&t),
            XmlEvent::End(_) => field.clear(),
            _ => {}
        }
    }
    let workbook = package
        .read_xml(host.loaded().workbook_part())
        .map_err(|e| e.to_string())?;
    let mut file_version = json!({});
    let mut reader = workbook.reader();
    while let Some(event) = reader.next_event().map_err(|e| e.to_string())? {
        if let XmlEvent::Start(e) = event
            && e.name.local == "fileVersion"
        {
            for attribute in e.attributes {
                if attribute.name.namespace.is_none() {
                    file_version[attribute.name.local.as_str()] = json!(attribute.value);
                }
            }
        }
    }
    info["metadata"] =
        json!({"application":application,"app_version":version,"file_version":file_version});
    if application != "Microsoft Excel"
        || version.is_empty()
        || file_version["appName"] != "xl"
        || file_version["rupBuild"].as_str().is_none_or(str::is_empty)
        || origin["application"] != application
        || origin["app_version"] != version
        || origin["file_version"] != file_version
        || !origin["evidence"].is_object()
    {
        info["unavailable"] =
            json!("Excel application/version metadata does not agree with the attested save");
        return Ok((record, info));
    }
    let mut caches = BTreeMap::new();
    let mut excluded = BTreeSet::new();
    let mut opaque = BTreeSet::new();
    for sheet in host.loaded().sheets() {
        let xml = package
            .read_xml(sheet.part().name())
            .map_err(|e| e.to_string())?;
        let mut cell = None;
        let mut kind = None;
        let mut value = None;
        let mut reading = false;
        let mut reader = xml.reader();
        while let Some(event) = reader.next_event().map_err(|e| e.to_string())? {
            match event {
                XmlEvent::Start(e) if e.name.local == "c" => {
                    cell = e
                        .attribute("r")
                        .and_then(a1)
                        .and_then(|(r, c)| CellKey::new(sheet.id(), r, c).ok());
                    kind = match e.attribute("t").unwrap_or("n") {
                        "n" => Some(CacheCellType::Number),
                        "b" => Some(CacheCellType::Logical),
                        "e" => Some(CacheCellType::Error),
                        "str" => Some(CacheCellType::Text),
                        _ => None,
                    };
                    value = None;
                    // vm can turn a raw #VALUE! token into #SPILL! or #CALC!.
                    // A lexical <v> alone does not reconstruct that rich value.
                    if e.attribute("vm").is_some() {
                        opaque.extend(cell);
                    }
                }
                XmlEvent::Start(e) if e.name.local == "f" => {
                    if e.attribute("ca").is_some_and(|v| v == "1" || v == "true")
                        || e.attribute("t") == Some("dataTable")
                    {
                        excluded.extend(cell);
                    }
                }
                XmlEvent::Start(e) if e.name.local == "v" => {
                    reading = true;
                    value = Some(String::new());
                }
                XmlEvent::Text(t) if reading => {
                    if let Some(v) = &mut value {
                        v.push_str(&t);
                    }
                }
                XmlEvent::End(e) if e.local == "v" => reading = false,
                XmlEvent::End(e) if e.local == "c" => {
                    if let Some((at, cache)) =
                        cell.zip(kind)
                            .zip(value.as_deref())
                            .and_then(|((at, kind), value)| {
                                SavedCache::new(kind, value).ok().map(|cache| (at, cache))
                            })
                    {
                        caches.insert(at, cache);
                    }
                    cell = None;
                }
                _ => {}
            }
        }
    }
    // Use the application's bound graph, including names and dynamic reads.
    // Exclude every reader of a volatile or explicitly dirty source, rather
    // than inspecting only the target's formula text.
    excluded.extend(host.volatile_formula_cells());
    excluded.extend(opaque.iter().copied());
    let mut pending: VecDeque<_> = excluded.iter().copied().collect();
    while let Some(at) = pending.pop_front() {
        if let Some(area) = host.spill_area(at) {
            for row in area.first().row()..=area.last().row() {
                for column in area.first().column()..=area.last().column() {
                    let child = CellKey::new(at.sheet(), row, column)
                        .expect("published spill is in the grid");
                    if excluded.insert(child) {
                        pending.push_back(child);
                    }
                }
            }
        }
        for reader in host.dependents(at) {
            if excluded.insert(reader) {
                pending.push_back(reader);
            }
        }
    }
    let live: BTreeSet<_> = capture
        .observations
        .iter()
        .filter(|o| matches!(o.observation, Observation::ExcelRaw(_)))
        .map(|o| &o.cell)
        .collect();
    let sheets: BTreeMap<_, _> = host
        .sheets()
        .iter()
        .map(|s| (s.name().text().to_string_lossy(), s.id()))
        .collect();
    let mut reasons = BTreeMap::<String, u64>::new();
    for formula in &capture.formulas {
        let at = sheets
            .get(formula.cell.sheet())
            .and_then(|id| CellKey::new(*id, formula.cell.row(), formula.cell.column()).ok());
        let reason = if live.contains(&formula.cell) {
            Some("live-answer-exists")
        } else if at.is_some_and(|at| opaque.contains(&at)) {
            Some("unclassified-value-metadata")
        } else if at.is_none_or(|at| excluded.contains(&at)) {
            Some("volatile-dirty-or-unclassified-dependency")
        } else if formula.dynamic_array
            || at.is_some_and(|at| host.spill_area(at).is_some_and(|a| a.cell_count() > 1))
        {
            Some("array-shape-not-a-scalar-cache")
        } else {
            None
        };
        if let Some(reason) = reason {
            *reasons.entry(reason.into()).or_default() += 1;
            continue;
        }
        if let Some(cache) = at.and_then(|at| caches.get(&at)) {
            record.observations.push(CellObservation {
                cell: formula.cell.clone(),
                observation: Observation::ExcelSavedCache(cache.clone()),
            });
        } else {
            *reasons.entry("no-typed-xml-value".into()).or_default() += 1;
        }
    }
    info["reference_cells"] = json!(record.observations.len());
    info["excluded"] = json!(reasons);
    Ok((record, info))
}

fn a1(text: &str) -> Option<(u32, u32)> {
    let split = text.find(|c: char| !c.is_ascii_uppercase())?;
    let (letters, digits) = text.split_at(split);
    let column = letters.bytes().try_fold(0u32, |n, b| {
        n.checked_mul(26)?.checked_add(u32::from(b - b'A' + 1))
    })?;
    Some((digits.parse().ok()?, column))
}
