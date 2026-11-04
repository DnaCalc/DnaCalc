//! Native file resolution for the portable calculation domain. Only local linked
//! packages are read; missing, unreadable and unopenable inputs keep labelled caches.
use calc_host::{
    CalculationDomain, DomainWorkbookId, HostOptions, HostState, MemorySource, OpenPolicy,
};
use serde_json::{Value, json};
use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
    sync::Arc,
};

fn local_path(text: &str) -> Option<PathBuf> {
    let text = if let Some(path) = text.strip_prefix("file://localhost") {
        path
    } else if let Some(path) = text.strip_prefix("file://") {
        if !path.starts_with('/') {
            return None;
        }
        path
    } else {
        text
    };
    let text = if text.as_bytes().get(2) == Some(&b':') && text.starts_with('/') {
        &text[1..]
    } else {
        text
    };
    if text.contains("://") {
        return None;
    }
    let bytes = text.as_bytes();
    let mut decoded = Vec::new();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' && i + 2 < bytes.len() {
            let high = (bytes[i + 1] as char).to_digit(16)?;
            let low = (bytes[i + 2] as char).to_digit(16)?;
            decoded.push((high * 16 + low) as u8);
            i += 3;
        } else if bytes[i] == b'%' {
            return None;
        } else {
            decoded.push(bytes[i]);
            i += 1;
        }
    }
    let text = String::from_utf8(decoded).ok()?.replace('\\', "/");
    Some(PathBuf::from(text))
}

fn resolve(
    parent: &Path,
    target: Option<&str>,
    absolute_target: Option<&str>,
    file_name: Option<&str>,
) -> Option<PathBuf> {
    let folder = parent.parent()?;
    for text in [target, absolute_target].into_iter().flatten() {
        if let Some(path) = local_path(text) {
            let candidate = if path.is_absolute() {
                path
            } else {
                folder.join(path)
            };
            if candidate.is_file() {
                return candidate.canonicalize().ok();
            }
        }
    }
    // Excel links commonly retain an obsolete absolute directory after a folder is
    // moved. Resolve a sibling of the referring package, never a repository-wide search.
    let name = file_name?;
    if name.contains(['/', '\\']) {
        return None;
    }
    folder
        .join(name)
        .canonicalize()
        .ok()
        .filter(|p| p.is_file())
}

pub(crate) fn load(
    domain: &mut CalculationDomain,
    root: DomainWorkbookId,
    fixture: &Path,
    options: &HostOptions,
    capture: &calc_verdict::CaptureRecord,
    trusted: Option<&crate::trusted_code::Manifest>, capture_digest: Option<&str>,
    owners: &mut Vec<crate::trusted_code::PreparedCode>,
) -> Vec<Value> {
    const MAX_WORKBOOKS: usize = 64;
    let mut records = Vec::new();
    let path = fixture
        .canonicalize()
        .unwrap_or_else(|_| fixture.to_owned());
    let mut seen = BTreeMap::from([(path.clone(), root)]);
    let mut pending = vec![(root, path)];
    while let Some((id, path)) = pending.pop() {
        let host = domain.host(id).expect("loaded workbook");
        let clock = host.entry_environment().now;
        let date1904 = host.recalc_context().date1904;
        let links: Vec<_> = host
            .model()
            .definitions()
            .external_books()
            .map(|(number, book)| (number, book.clone()))
            .collect();
        for (number, book) in links {
            let Some(source_path) = resolve(
                &path,
                book.target.as_deref(),
                book.absolute_target.as_deref(),
                book.file_name.as_deref(),
            ) else {
                records.push(json!({"referrer":path,"link":number,"target":book.target,"status":"external-cache: source unavailable"}));
                continue;
            };
            let source = if let Some(&source) = seen.get(&source_path) {
                source
            } else {
                if seen.len() >= MAX_WORKBOOKS {
                    records.push(json!({"referrer":path,"link":number,"source":source_path,"status":"external-cache: workbook limit"}));
                    continue;
                }
                let opened = std::fs::read(&source_path)
                    .map_err(|e| e.to_string())
                    .and_then(|bytes| {
                        let hash = calc_verdict::sha256_hex(&bytes);
                        let memory = MemorySource::new(bytes).named(
                            source_path
                                .file_name()
                                .and_then(|n| n.to_str())
                                .unwrap_or("linked.xlsx"),
                        );
                        HostState::open(Arc::new(memory), &OpenPolicy::default(), options.clone())
                            .map(|host| (host, hash))
                            .map_err(|e| e.to_string())
                    });
                let (mut source, hash) = match opened {
                    Ok(source) => source,
                    Err(error) => {
                        records.push(json!({"referrer":path,"link":number,"source":source_path,"status":"external-cache: open failed","reason":error}));
                        continue;
                    }
                };
                let source1904 = source.recalc_context().date1904;
                source.set_clock(clock.map(|clock| {
                    clock + if date1904 { 1462.0 } else { 0.0 }
                        - if source1904 { 1462.0 } else { 0.0 }
                }));
                let mut session = calc_host::HostSession::new(source);
                if let Some(trusted) = trusted {
                    match trusted.prepare(session.host(),source_path.file_name().and_then(|n|n.to_str()).unwrap_or("linked.xlsx"),capture,&hash,capture_digest,true) {
                        Ok(code) => { code.install(&mut session); owners.push(code); }
                        Err(error) => {
                            records.push(json!({"referrer":path,"link":number,"source":source_path,"sha256":hash,"execution_policy":trusted.policy(&hash),"status":"external-cache: trusted code refused","key":error.key,"reason":error.message}));
                            continue;
                        }
                    }
                } else { match crate::compare::prepare_captured_vba(
                    session.host(),
                    source_path.file_name().and_then(|name| name.to_str()).unwrap_or("linked.xlsx"),
                    capture,
                ) {
                    Ok(Some((project, events))) => {
                        session.set_workbook_events_enabled(events);
                        project.install(&mut session);
                        session.notify_workbook_open();
                    }
                    Ok(None) => {}
                    Err(error) => {
                        records.push(json!({"referrer":path,"link":number,"source":source_path,"status":"external-cache: VBA project refused","reason":error.message}));
                        continue;
                    }
                }
                }
                let source = domain
                    .insert_session(session)
                    .expect("loader has no active calculation");
                seen.insert(source_path.clone(), source);
                pending.push((source, source_path.clone()));
                records.push(json!({"source":source_path,"sha256":hash,"status":"opened for engine calculation"}));
                source
            };
            domain
                .connect(id, number, source)
                .expect("loader connects declared links");
            records.push(
                json!({"referrer":path,"link":number,"source":source_path,"status":"connected"}),
            );
        }
    }
    records
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn local_file_uris_and_percent_escapes_do_not_fetch_remote_resources() {
        assert_eq!(
            local_path("file:///tmp/a%20b.xlsx"),
            Some(PathBuf::from("/tmp/a b.xlsx"))
        );
        assert_eq!(
            local_path("file:///C:/Data/a.xlsx"),
            Some(PathBuf::from("C:/Data/a.xlsx"))
        );
        assert_eq!(
            local_path("file://localhost/tmp/a.xlsx"),
            Some(PathBuf::from("/tmp/a.xlsx"))
        );
        for text in [
            "file://server/a.xlsx",
            "https://example.org/a.xlsx",
            "%€",
            "%",
            "%GG",
        ] {
            assert_eq!(local_path(text), None, "{text}");
        }
    }
}
