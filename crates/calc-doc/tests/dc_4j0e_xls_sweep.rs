//! dc-4j0e: the read sweep over legacy Excel files. Native-only: it reads the file system.
//!
//! `DNACALC_XLS_SWEEP` is a list of files and directories (separated by `:`); every OLE2 file
//! under them (by its first eight bytes, whatever its extension) is opened and loaded. Files are
//! data: they are read, never executed or opened in Office. The test prints the progress table:
//! files opened, refused by reason, load refusals, panics, and the record types the converter
//! did not read, most frequent first. It fails only on a panic. Run:
//!
//! `DNACALC_XLS_PASSWORD` (optional) is tried on every encrypted file.
//!
//! `DNACALC_XLS_SWEEP=/path/a:/path/b cargo test -p calc-doc --test dc_4j0e_xls_sweep -- --nocapture`

use calc_doc::model::value::{IdAllocator, SheetId};
use calc_doc::{MemorySource, OpenError, OpenPolicy, PackageRead, legacy_workbook_report};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

fn collect(path: &Path, out: &mut Vec<PathBuf>) {
    if path.is_dir() {
        if let Ok(read) = std::fs::read_dir(path) {
            let mut children: Vec<_> = read.flatten().map(|e| e.path()).collect();
            children.sort();
            for child in children {
                collect(&child, out);
            }
        }
    } else if path.is_file() {
        out.push(path.to_owned());
    }
}

/// Native-only: reads the files named by `DNACALC_XLS_SWEEP`.
#[test]
fn sweep() {
    let Ok(list) = std::env::var("DNACALC_XLS_SWEEP") else {
        println!("DNACALC_XLS_SWEEP is not set: nothing to sweep");
        return;
    };
    let mut files = Vec::new();
    for item in list.split(':').filter(|s| !s.is_empty()) {
        collect(Path::new(item), &mut files);
    }
    let mut seen = 0usize;
    let mut opened = 0usize;
    let mut loaded = 0usize;
    let (mut formula_cells, mut parse_failures) = (0usize, 0usize);
    let audit_phonetic = std::env::var_os("DNACALC_XLS_AUDIT_PHONETIC").is_some();
    let audit_pivots = std::env::var_os("DNACALC_XLS_AUDIT_PIVOT_CACHE").is_some();
    let audit_charts = std::env::var_os("DNACALC_XLS_AUDIT_CHART_CACHE").is_some();
    let audit_chart_sheets = std::env::var_os("DNACALC_XLS_AUDIT_CHART_SHEETS").is_some();
    let audit_core = std::env::var_os("DNACALC_XLS_AUDIT_CORE_PROPERTIES").is_some();
    let (mut core_files, mut core_titles, mut core_authors, mut core_modifiers) = (0usize, 0usize, 0usize, 0usize);
    let (mut core_created, mut core_modified) = (0usize, 0usize);
    let (mut core_categories, mut app_files, mut app_names, mut app_managers, mut app_companies) = (0usize, 0usize, 0usize, 0usize, 0usize);
    let (mut custom_files, mut custom_count) = (0usize, 0usize);
    let (mut chart_sheets, mut chart_sheet_files) = (0usize, 0usize);
    let (mut chart_points, mut chart_files) = (0usize, 0usize);
    let (mut category_points, mut category_files) = (0usize, 0usize);
    let (mut series_fills, mut series_fill_files) = (0usize, 0usize);
    let (mut pivot_caches, mut pivot_records, mut pivot_files) = (0usize, 0u64, 0usize);
    let (mut phonetic_runs, mut phonetic_files, mut phonetic_errors) = (0usize, 0usize, 0usize);
    let (mut phonetic_formats, mut phonetic_format_files) = (0usize, 0usize);
    let mut refused: BTreeMap<String, usize> = BTreeMap::new();
    let mut load_refused: BTreeMap<String, usize> = BTreeMap::new();
    let mut panics: Vec<String> = Vec::new();
    let mut unknown: BTreeMap<u16, (u32, usize)> = BTreeMap::new();
    let mut unread_custom: BTreeMap<u16, (u32, usize)> = BTreeMap::new();
    let mut unread_custom_sets = 0u32;
    let mut shown_refusals: BTreeMap<String, String> = BTreeMap::new();
    let mut shown_load_refusals: BTreeMap<String, String> = BTreeMap::new();
    std::panic::set_hook(Box::new(|_| {}));
    for file in &files {
        let Ok(bytes) = std::fs::read(file) else { continue };
        if bytes.len() < 8 || bytes[..8] != [0xD0, 0xCF, 0x11, 0xE0, 0xA1, 0xB1, 0x1A, 0xE1] {
            continue;
        }
        seen += 1;
        let source: Arc<dyn PackageRead> = Arc::new(MemorySource::new(bytes));
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let policy = OpenPolicy::default();
            let report = legacy_workbook_report(source.clone(), &policy);
            let password = std::env::var("DNACALC_XLS_PASSWORD").ok();
            let document = calc_doc::open_with_password(source, &policy, password.as_deref());
            let core = if audit_core {
                document.as_ref().ok().and_then(|doc| doc.package().read_part("docProps/core.xml").ok()).map(|xml| {
                    let has = |tag: &[u8]| xml.windows(tag.len()).any(|window| window == tag);
                    (has(b"<dc:title>"), has(b"<dc:creator>"), has(b"<cp:lastModifiedBy>"),
                        has(b"<dcterms:created "), has(b"<dcterms:modified "), has(b"<cp:category>"))
                })
            } else { None };
            let app = if audit_core {
                document.as_ref().ok().and_then(|doc| doc.package().read_part("docProps/app.xml").ok()).map(|xml| {
                    let has = |tag: &[u8]| xml.windows(tag.len()).any(|window| window == tag);
                    (has(b"<Application>"), has(b"<Manager>"), has(b"<Company>"))
                })
            } else { None };
            let custom = if audit_core {
                document.as_ref().ok().and_then(|doc| doc.package().read_part("docProps/custom.xml").ok())
                    .map(|xml| xml.windows(10).filter(|window| *window == b"<property ").count())
            } else { None };
            let written_chart_sheets = if audit_chart_sheets {
                document.as_ref().ok().map(|doc| doc.package().parts().iter()
                    .filter(|part| part.name().starts_with("xl/chartsheets/sheet") && part.name().ends_with(".xml"))
                    .filter(|part| doc.package().read_part(part.name()).is_ok_and(|xml| xml.windows(8).any(|w| w == b"<drawing")))
                    .count()).unwrap_or(0)
            } else { 0 };
            let written_phonetic = if audit_phonetic {
                document.as_ref().ok().and_then(|doc| {
                    if doc.package().part("xl/sharedStrings.xml").is_none() { return Some((0, 0)); }
                    doc.package().read_part("xl/sharedStrings.xml").ok()
                        .map(|xml| (xml.windows(5).filter(|window| *window == b"<rPh ").count(),
                            xml.windows(12).filter(|window| *window == b"<phoneticPr ").count()))
                })
            } else { Some((0, 0)) };
            let load = document.as_ref().ok().map(|d| {
                let mut ids: IdAllocator<SheetId> = IdAllocator::new();
                calc_doc::load(d, &mut ids).map_err(|e| e.to_string()).map(|loaded| {
                    // Every formula must re-parse in calc-fml.
                    let (mut formulas, mut failed) = (0usize, 0usize);
                    for sheet in loaded.model().sheets() {
                        for (_, input) in loaded.model().sheet_cells(sheet.id()).into_iter().flatten() {
                            if let calc_doc::model::CellInput::Formula(f) = input {
                                formulas += 1;
                                let text = f.rendered().to_string_lossy();
                                if !calc_fml::parse_formula(&text).diagnostics().is_empty() {
                                    failed += 1;
                                }
                            }
                        }
                    }
                    let caches = if audit_pivots { loaded.pivot_caches().caches.len() } else { 0 };
                    let saved_records = if audit_pivots {
                        loaded.pivot_caches().caches.values().filter_map(|c| c.record_count).sum()
                    } else { 0 };
                    let points = if audit_charts {
                        loaded.sheets().iter().flat_map(|s| &s.charts().charts)
                            .flat_map(|c| &c.plots).flat_map(|p| &p.series)
                            .filter_map(|s| s.values.as_ref()).filter(|v| v.numeric)
                            .map(|v| v.cache.iter().flatten().count()).sum()
                    } else { 0 };
                    let categories = if audit_charts {
                        loaded.sheets().iter().flat_map(|s| &s.charts().charts)
                            .flat_map(|c| &c.plots).flat_map(|p| &p.series)
                            .filter_map(|s| s.categories.as_ref())
                            .map(|v| v.cache.iter().flatten().count()).sum()
                    } else { 0 };
                    let fills = if audit_charts {
                        loaded.sheets().iter().flat_map(|s| &s.charts().charts)
                            .flat_map(|c| &c.plots).flat_map(|p| &p.series)
                            .filter(|s| s.fill.is_some()).count()
                    } else { 0 };
                    (formulas, failed, caches, saved_records, points, categories, fills)
                })
            });
            (report, document.map(|_| ()), load, written_phonetic, written_chart_sheets, core, app, custom)
        }));
        match result {
            Err(_) => panics.push(file.display().to_string()),
            Ok((report, document, load, written_phonetic, written_chart_sheets, core, app, custom)) => {
                match document {
                    Err(OpenError::LegacyWorkbook { detail, .. }) => {
                        let key = detail.chars().take(70).collect::<String>();
                        *refused.entry(key.clone()).or_insert(0) += 1;
                        shown_refusals.entry(key).or_insert_with(|| file.display().to_string());
                    }
                    Err(other) => {
                        let key: String = other.to_string().chars().take(70).collect();
                        *refused.entry(key.clone()).or_insert(0) += 1;
                        shown_refusals.entry(key).or_insert_with(|| file.display().to_string());
                    }
                    Ok(()) => {
                        opened += 1;
                        if let Some((title, author, modifier, created, modified, category)) = core {
                            core_files += 1; core_titles += usize::from(title);
                            core_authors += usize::from(author); core_modifiers += usize::from(modifier);
                            core_created += usize::from(created); core_modified += usize::from(modified);
                            core_categories += usize::from(category);
                        }
                        if let Some((name, manager, company)) = app {
                            app_files += 1; app_names += usize::from(name); app_managers += usize::from(manager);
                            app_companies += usize::from(company);
                        }
                        if let Some(count) = custom { custom_files += 1; custom_count += count; }
                        chart_sheets += written_chart_sheets;
                        chart_sheet_files += usize::from(written_chart_sheets != 0);
                        if let Some((runs, formats)) = written_phonetic {
                            phonetic_runs += runs;
                            phonetic_files += usize::from(runs != 0);
                            phonetic_formats += formats;
                            phonetic_format_files += usize::from(formats != 0);
                        } else if audit_phonetic {
                            phonetic_errors += 1;
                        }
                        match load {
                            Some(Ok((formulas, failed, caches, saved_records, points, categories, fills))) => {
                                loaded += 1;
                                formula_cells += formulas;
                                parse_failures += failed;
                                pivot_caches += caches;
                                pivot_records += saved_records;
                                pivot_files += usize::from(caches != 0);
                                chart_points += points;
                                chart_files += usize::from(points != 0);
                                category_points += categories;
                                category_files += usize::from(categories != 0);
                                series_fills += fills;
                                series_fill_files += usize::from(fills != 0);
                            }
                            Some(Err(e)) => {
                                let key: String = e.chars().take(90).collect();
                                *load_refused.entry(key.clone()).or_insert(0) += 1;
                                shown_load_refusals.entry(key).or_insert_with(|| file.display().to_string());
                            }
                            None => {}
                        }
                        if let Ok(Some(report)) = report {
                            unread_custom_sets += report.unread_custom_property_sets;
                            for (kind, n) in report.unread_custom_property_types {
                                let slot = unread_custom.entry(kind).or_insert((0, 0));
                                slot.0 += n; slot.1 += 1;
                            }
                            for (id, n) in report.unknown_records {
                                let slot = unknown.entry(id).or_insert((0, 0));
                                slot.0 += n;
                                slot.1 += 1;
                            }
                        }
                    }
                }
            }
        }
    }
    println!("OLE2 files seen:      {seen}");
    println!("opened (converted):   {opened}");
    println!("loaded (model built): {loaded}");
    println!("formula cells loaded: {formula_cells}, not re-parsed by calc-fml: {parse_failures}");
    if audit_phonetic {
        println!("phonetic runs written: {phonetic_runs} in {phonetic_files} files; audit errors: {phonetic_errors} (not an Excel match verdict)");
        println!("shared-string phonetic formats written: {phonetic_formats} in {phonetic_format_files} files (not an Excel match verdict)");
    }
    if audit_pivots {
        println!("saved pivot source caches: {pivot_caches} in {pivot_files} files, {pivot_records} declared source records (not pivot views or calculated results)");
    }
    if audit_charts {
        println!("numeric chart display-cache points: {chart_points} in {chart_files} files (not calculated cell results)");
        println!("chart category display-cache points: {category_points} in {category_files} files (not calculated cell results)");
        println!("chart series with explicit solid fill: {series_fills} in {series_fill_files} files (not an Excel match verdict)");
    }
    if audit_chart_sheets {
        println!("chart sheets with a linked chart: {chart_sheets} in {chart_sheet_files} files (partial chart conversion, not an Excel match verdict)");
    }
    if audit_core {
        println!("core metadata parts: {core_files} files; title {core_titles}, author {core_authors}, last saved by {core_modifiers}, created {core_created}, modified {core_modified}, category {core_categories} (not an Excel match verdict)");
        println!("extended metadata parts: {app_files} files; application {app_names}, manager {app_managers}, company {app_companies} (not an Excel match verdict)");
        println!("custom scalar metadata: {custom_count} properties in {custom_files} files (not an Excel match verdict)");
    }
    println!("refused to open, by reason:");
    for (reason, n) in &refused {
        println!("  {n:5}  {reason}   e.g. {}", shown_refusals[reason]);
    }
    println!("opened but refused to load, by reason:");
    for (reason, n) in &load_refused {
        println!("  {n:5}  {reason}   e.g. {}", shown_load_refusals[reason]);
    }
    println!("panics: {}", panics.len());
    for p in &panics {
        println!("  {p}");
    }
    println!("unread custom property sets: {unread_custom_sets}");
    println!("unread custom property value types (type: values, files):");
    for (kind, (count, files)) in &unread_custom {
        println!("  0x{kind:04X}: {count} values in {files} files");
    }
    let mut by_count: Vec<_> = unknown.iter().collect();
    by_count.sort_by_key(|(_, (n, files))| std::cmp::Reverse((*files, *n)));
    println!("record types not read (type: records, files):");
    for (id, (n, files)) in by_count {
        println!("  0x{id:04X}: {n} records in {files} files");
    }
    assert!(panics.is_empty(), "the reader panicked on {} files", panics.len());
}
