use crate::query::QueryWork;
use calc_doc_model::{ExternalBook, SheetKind, Table};
use calc_host::HostState;
use calc_skin::grid::{Area, Sheet};
use calc_skin::{
    ExternalLink, InventoryItem, InventoryKind, NameScope, Refusal, ScenarioRef, SheetVisibility, SourceRef,
    TableInfo,
};

pub(crate) fn table_info(table: &Table) -> TableInfo {
    TableInfo {
        name: table.display_name.clone(),
        area: Area::of(table.area),
        headers: table.header_rows,
        totals: table.totals_rows,
        columns: table.columns.clone(),
    }
}

pub(crate) fn link_info(index: u32, link: &ExternalBook) -> ExternalLink {
    ExternalLink {
        index,
        file_name: link.file_name.clone(),
        cached_sheets: link.sheets.iter().filter(|sheet| sheet.cached).count() as u64,
        refresh_error_sheets: link
            .sheets
            .iter()
            .filter(|sheet| sheet.refresh_error)
            .count() as u64,
    }
}

pub(crate) fn name_scope(scope: calc_doc_model::NameScope) -> NameScope {
    match scope {
        calc_doc_model::NameScope::Workbook => NameScope::Workbook,
        calc_doc_model::NameScope::Sheet(sheet) => NameScope::Sheet(Sheet::of(sheet)),
    }
}

pub(crate) fn inventory(
    host: &HostState,
    kinds: &[InventoryKind],
    meter: &mut QueryWork,
) -> Result<Vec<InventoryItem>, Refusal> {
    let definitions = host.model().definitions();
    let wanted = |kind| kinds.is_empty() || kinds.contains(&kind);
    let mut items = Vec::new();
    let mut add = |item| {
        meter.work(0, 1, 0)?;
        items.push(item);
        Ok(())
    };
    for sheet in host.sheets() {
        let id = Sheet::of(sheet.id());
        if wanted(InventoryKind::Sheets) {
            let visibility = match sheet.visibility() {
                calc_doc_model::SheetVisibility::Visible => SheetVisibility::Visible,
                calc_doc_model::SheetVisibility::Hidden => SheetVisibility::Hidden,
                calc_doc_model::SheetVisibility::VeryHidden => SheetVisibility::VeryHidden,
            };
            add(InventoryItem::Sheet {
                sheet: id,
                name: sheet.name().text().to_string_lossy(),
                visibility,
                worksheet: sheet.kind() == SheetKind::Worksheet,
            })?;
        }
        let object = |name: &str| SourceRef::Object {
            sheet: id,
            name: name.to_owned(),
        };
        if wanted(InventoryKind::Charts) {
            for chart in definitions.charts(sheet.id()) {
                add(InventoryItem::Chart {
                    object: object(&chart.name),
                })?;
            }
        }
        if wanted(InventoryKind::Shapes) {
            for shape in definitions.shapes(sheet.id()) {
                add(InventoryItem::Shape {
                    object: object(&shape.name),
                })?;
            }
        }
        if wanted(InventoryKind::Scenarios) {
            for scenario in worksheet_scenarios(host, id) {
                add(InventoryItem::Scenario(ScenarioRef {
                    sheet: id,
                    name: scenario.name.clone(),
                }))?;
            }
        }
    }
    if wanted(InventoryKind::Names) {
        for name in definitions.name_table().definitions() {
            add(InventoryItem::Name {
                scope: name_scope(name.scope),
                name: name.name.clone(),
                formula: calc_formula_kernel::display::formula(&format!("={}", name.formula)),
                hidden: name.hidden,
            })?;
        }
    }
    if wanted(InventoryKind::Tables) {
        for table in definitions.tables() {
            add(InventoryItem::Table(table_info(table)))?;
        }
    }
    if wanted(InventoryKind::PivotTables) {
        for pivot in definitions.pivots().tables() {
            add(InventoryItem::PivotTable {
                name: pivot.name.clone(),
                area: Area::of(pivot.area),
            })?;
        }
    }
    if wanted(InventoryKind::ExternalLinks) {
        for (index, link) in definitions.external_books() {
            add(InventoryItem::ExternalLink(link_info(index, link)))?;
        }
    }
    if wanted(InventoryKind::VbaProject) && host.loaded().has_vba_project() {
        add(InventoryItem::VbaProject)?;
    }
    Ok(items)
}

pub(crate) fn worksheet_scenarios(
    host: &HostState,
    sheet: Sheet,
) -> impl Iterator<Item = &calc_doc_model::WorksheetScenario> {
    let id = sheet.id();
    let visible = host.model().sheet_by_id(id).is_some_and(|entry| {
        entry.kind() == SheetKind::Worksheet
            && entry.visibility() == calc_doc_model::SheetVisibility::Visible
    });
    host.model().definitions().scenarios(id)
        .into_iter()
        .flat_map(|records| &records.entries)
        .filter(move |scenario| visible && (!scenario.hidden || !host.sheet_contents_protected(id)))
}

pub(crate) fn sources(
    host: &HostState,
    meter: &mut QueryWork,
) -> Result<calc_skin::Sources, Refusal> {
    let mut answer = calc_skin::Sources {
        links: Vec::new(),
        topics: Vec::new(),
        volatile_cells: Vec::new(),
    };
    for (index, link) in host.model().definitions().external_books() {
        meter.work(0, 1, 0)?;
        answer.links.push(link_info(index, link));
    }
    for (topic, entry) in host.rtd_topics() {
        meter.work(0, 1, 0)?;
        let receipt = host.rtd_receipt(topic, entry.lease).map(|receipt| calc_skin::LiveTopicReceipt {
            received: receipt.received,
            timed_received: receipt.timed_received,
            first_unix_ms: receipt.first_unix_ms,
            last_unix_ms: receipt.last_unix_ms,
        });
        answer.topics.push(calc_skin::LiveTopic {
            progid: topic.progid.clone(),
            server: topic.server.clone(),
            strings: topic.strings.clone(),
            lease: entry.lease.0,
            sequence: entry.sequence,
            receipt,
            last: calc_skin::Shown {
                value: crate::query::raw(&entry.value),
                provenance: calc_skin::Provenance::External {
                    source: topic.progid.clone(),
                    observed_at: receipt.and_then(|receipt| receipt.last_unix_ms),
                },
            },
        });
    }
    for at in host.volatile_formula_anchors() {
        meter.work(1, 0, 0)?;
        answer.volatile_cells.push(calc_skin::grid::Key::of(at));
    }
    answer.volatile_cells.sort_unstable();
    Ok(answer)
}
