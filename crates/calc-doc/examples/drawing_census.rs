//! dc-zjsg.1 corpus measure. Walks the directories given as arguments for `.xlsx` and `.xlsm`
//! files, reads every sheet's drawing objects (`LoadedSheet::shapes()`) and reports, per kind,
//! how many objects the reader holds and how many anchors it keeps in the package unread, then
//! checks that an untouched save keeps every drawing part byte for byte.
//!
//! Native-only tool: `cargo run -p calc-doc --example drawing_census -- [--every N] <dir>...`.
//! `--every N` reads every Nth file in path order, a deterministic sample. The files are read,
//! never changed; nothing is written.

use calc_doc::model::value::{IdAllocator, SheetId};
use calc_doc::model::ShapeKind;
use calc_doc::{MemorySource, OpenPolicy, PackageRead, SavePolicy};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

fn collect(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    let mut entries: Vec<_> = entries.flatten().map(|e| e.path()).collect();
    entries.sort();
    for path in entries {
        if path.is_dir() {
            collect(&path, out);
        } else if path
            .extension()
            .and_then(|e| e.to_str())
            .is_some_and(|e| e.eq_ignore_ascii_case("xlsx") || e.eq_ignore_ascii_case("xlsm"))
        {
            out.push(path);
        }
    }
}

fn drawing_parts(document: &calc_doc::OpenedDocument) -> Vec<String> {
    document
        .package()
        .parts()
        .iter()
        .map(|p| p.name().to_owned())
        .filter(|n| n.to_ascii_lowercase().starts_with("xl/drawings/") && n.ends_with(".xml"))
        .collect()
}

fn main() {
    let mut files = Vec::new();
    let mut every = 1usize;
    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        if arg == "--every" {
            every = args.next().and_then(|n| n.parse().ok()).unwrap_or(1).max(1);
        } else {
            collect(Path::new(&arg), &mut files);
        }
    }
    let found = files.len();
    files = files.into_iter().step_by(every).collect();
    let (mut opened, mut unopened, mut loaded_ok, mut with_drawing) = (0usize, 0usize, 0usize, 0usize);
    let (mut sheets_with_objects, mut unread_total, mut anchors_total) = (0usize, 0usize, 0usize);
    let mut by_kind: BTreeMap<&'static str, usize> = BTreeMap::new();
    let mut by_geometry: BTreeMap<String, usize> = BTreeMap::new();
    let mut unread_by: BTreeMap<String, usize> = BTreeMap::new();
    let (mut round_trips, mut round_trip_failures, mut parts_checked) = (0usize, 0usize, 0usize);
    let mut failures = Vec::new();
    for (n, path) in files.iter().enumerate() {
        if n % 250 == 0 {
            eprintln!("{n} of {} files: {}", files.len(), path.display());
        }
        let Ok(bytes) = std::fs::read(path) else {
            unopened += 1;
            continue;
        };
        let source: Arc<dyn PackageRead> = Arc::new(MemorySource::new(bytes));
        let Ok(document) = calc_doc::open(source, &OpenPolicy::default()) else {
            unopened += 1;
            continue;
        };
        opened += 1;
        let parts = drawing_parts(&document);
        let mut ids: IdAllocator<SheetId> = IdAllocator::new();
        let Ok(loaded) = calc_doc::load(&document, &mut ids) else {
            continue;
        };
        loaded_ok += 1;
        let mut file_objects = 0usize;
        for sheet in loaded.sheets() {
            let read = sheet.shapes();
            if !read.shapes.is_empty() || read.unread > 0 {
                sheets_with_objects += 1;
            }
            file_objects += read.shapes.len() + read.unread;
            unread_total += read.unread;
            for (label, n) in &read.unread_by {
                *unread_by.entry(label.clone()).or_default() += n;
            }
            anchors_total += read.shapes.len() + read.unread;
            fn count(
                shapes: &[calc_doc::model::Shape],
                by_kind: &mut BTreeMap<&'static str, usize>,
                by_geometry: &mut BTreeMap<String, usize>,
            ) {
                for s in shapes {
                    *by_kind
                        .entry(match s.kind {
                            ShapeKind::Shape => "shape",
                            ShapeKind::TextBox => "text box",
                            ShapeKind::Picture => "picture",
                            ShapeKind::Connector => "connector",
                            ShapeKind::Group => "group",
                        })
                        .or_default() += 1;
                    if let Some(g) = &s.geometry {
                        *by_geometry.entry(g.clone()).or_default() += 1;
                    }
                    count(&s.children, by_kind, by_geometry);
                }
            }
            count(&read.shapes, &mut by_kind, &mut by_geometry);
        }
        if parts.is_empty() {
            continue;
        }
        with_drawing += 1;
        let _ = file_objects;
        // An untouched save keeps every drawing part as stored.
        let saved = (|| {
            let plan = calc_doc::prepare_save(&document.snapshot(), &SavePolicy::default()).ok()?;
            let mut out = Vec::new();
            calc_doc::write(plan, &mut out).ok()?;
            let again: Arc<dyn PackageRead> = Arc::new(MemorySource::new(out));
            calc_doc::open(again, &OpenPolicy::default()).ok()
        })();
        match saved {
            Some(saved) => {
                let identical = parts.iter().all(|name| {
                    parts_checked += 1;
                    document.package().read_part(name).ok() == saved.package().read_part(name).ok()
                });
                if identical {
                    round_trips += 1;
                } else {
                    round_trip_failures += 1;
                    failures.push(format!("{}: a drawing part changed", path.display()));
                }
            }
            None => {
                round_trip_failures += 1;
                failures.push(format!("{}: the untouched save failed", path.display()));
            }
        }
    }
    println!("files found / read:           {found} / {} (every {every})", files.len());
    println!("opened / not opened:         {opened} / {unopened}");
    println!("loaded:                      {loaded_ok}");
    println!("with a drawing part:         {with_drawing}");
    println!("sheets with drawing objects: {sheets_with_objects}");
    println!("anchors (read + unread):     {anchors_total}");
    println!("read, by kind:");
    for (kind, n) in &by_kind {
        println!("  {kind:10} {n}");
    }
    println!("unread (kept in the package): {unread_total}");
    for (label, n) in &unread_by {
        println!("  {label:60} {n}");
    }
    println!("preset geometries read, top 15:");
    let mut geometries: Vec<_> = by_geometry.into_iter().collect();
    geometries.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(&b.0)));
    for (g, n) in geometries.into_iter().take(15) {
        println!("  {g:20} {n}");
    }
    println!(
        "untouched save keeps drawing parts byte for byte: {round_trips} of {with_drawing} files ({parts_checked} parts), {round_trip_failures} failures"
    );
    for failure in failures.iter().take(20) {
        println!("  {failure}");
    }
}
