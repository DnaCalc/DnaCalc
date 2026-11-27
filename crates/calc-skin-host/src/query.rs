use crate::{
    AnswerResult, State, batch, cell, contributions, dependencies, grid_area, invalid, inventory,
    issues, structure, survey, unavailable,
};
use calc_doc_model::{SheetKind, SheetVisibility};
use calc_host::value::{CalcValue, CellKey, CoreValue, GridArea};
use calc_host::{HostState, ProjectedCell, ProjectedValue, ProjectionRequest, search::entry_text};
use calc_skin::grid::{Area, Key, Sheet};
use calc_skin::query::Query;
use calc_skin::*;
use serde::Serialize;
use serde_json::Value;
use std::collections::BTreeSet;

const MAX_QUERY_CELLS: u64 = 65_536;

pub(crate) fn capabilities(state: &State) -> Capabilities {
    Capabilities {
        read_only: state.read_only,
        supported: vec![Capability::Workbook, Capability::Printing],
        platform_reserved_chords: Vec::new(),
    }
}

fn chart_image(
    state: &State,
    object: &SourceRef,
    bound: Option<&Bound>,
    meter: &mut QueryWork,
) -> Result<ChartImage, Refusal> {
    use base64::Engine as _;
    let SourceRef::Object { sheet, name } = object else {
        return Err(invalid("Choose a worksheet chart"));
    };
    let chart = state.session.host().chart_named(sheet.id(), name)
        .ok_or_else(|| invalid("Source missing"))?;
    let mut uses_saved_values = false;
    for series in chart.plots.iter().flat_map(|plot| &plot.series) {
        let points = series.values.len().max(series.categories.len()).max(series.sizes.len());
        meter.work(points as u64, 1, 0)?;
        uses_saved_values |= series.cached;
    }
    let overlay = if let Some(previous) = bound.and_then(|bound| crate::comparison::chart(state, bound, object)) {
        for series in previous.plots.iter().flat_map(|plot| &plot.series) {
            meter.work(series.values.len().max(series.categories.len()) as u64, 1, 0)?;
        }
        calc_chart_render::chart_comparison_svg(&chart, previous, 640.0, 360.0)
    } else {
        None
    };
    let compares_baseline = overlay.is_some();
    let svg = overlay.unwrap_or_else(|| calc_chart_render::chart_svg(&chart, 640.0, 360.0));
    let data = base64::engine::general_purpose::STANDARD.encode(svg);
    Ok(ChartImage {
        label: chart.title.filter(|title| !title.is_empty()).unwrap_or(chart.name),
        data_url: format!("data:image/svg+xml;base64,{data}"),
        uses_saved_values,
        compares_baseline,
    })
}

pub(crate) fn checked_sheet(
    host: &HostState,
    sheet: Sheet,
) -> Result<calc_host::value::SheetId, Refusal> {
    let id = sheet.id();
    host.model()
        .sheet_by_id(id)
        .ok_or_else(|| invalid("The worksheet is unavailable"))?;
    Ok(id)
}

pub(crate) fn raw(value: &CalcValue) -> RawValue {
    match value.core() {
        CoreValue::Number(value) => RawValue::Number {
            bits: value.to_bits(),
        },
        CoreValue::Text(value) => RawValue::Text(value.utf16_code_units().to_vec()),
        CoreValue::Logical(value) => RawValue::Logical(*value),
        CoreValue::Error(code) => RawValue::Error(code.excel_spelling().into()),
        CoreValue::Empty | CoreValue::Missing => RawValue::Blank,
        CoreValue::Array(array) => array.get(0, 0).map_or(RawValue::Blank, raw),
        CoreValue::Reference(_) => RawValue::Other {
            kind: "reference".into(),
            display: String::new(),
        },
    }
}

pub(crate) fn result_observation(result: Option<&calc_host::FormulaResult>) -> query::FormulaObservation {
    use calc_host::FormulaResult::{Calculated, FileCached, NotModeled};
    use query::ResultOrigin;

    let (value, origin) = match result {
        None => (None, ResultOrigin::None),
        Some(FileCached(cache)) => (Some(raw(&cache.value().to_value())), ResultOrigin::AsSaved),
        Some(Calculated { value, calculation }) => {
            let tick = calculation.tick().get();
            let origin = if calculation.held_by_circular_rule() {
                ResultOrigin::CircularHold { tick }
            } else if calculation.reads_external_links() {
                ResultOrigin::ExternalCache { tick }
            } else {
                ResultOrigin::Calculated { tick }
            };
            (Some(raw(value)), origin)
        }
        Some(NotModeled { calculation, .. }) => (
            None,
            ResultOrigin::Refused { tick: calculation.tick().get() },
        ),
    };
    query::FormulaObservation { value, origin }
}

pub(crate) fn anchor_source(state: &State, id: AnchorId) -> Result<SourceRef, Refusal> {
    let host = state.session.host();
    let anchor = host.model()
        .view_part()
        .anchor(id)
        .ok_or_else(|| invalid("The anchor does not exist"))?;
    if let Some((key, name)) = anchor.object_target() {
        let sheet = host.model().sheet_by_key(key)
            .filter(|sheet| host.has_named_chart(sheet.id(), name))
            .ok_or_else(|| invalid("The chart is unavailable"))?;
        return Ok(SourceRef::Object { sheet: Sheet::of(sheet.id()), name: name.into() });
    }
    let area = anchor
        .area()
        .ok_or_else(|| invalid("The anchor is orphaned"))?;
    Ok(if anchor.spill {
        SourceRef::Spill {
            anchor: Key::of(area.first()),
        }
    } else {
        batch::source_area(area)
    })
}

pub(crate) fn bound_source(state: &State, bound: &Bound) -> Result<SourceRef, Refusal> {
    let mut book = address_book(state);
    // A stored binding cannot take its sheet from the current selection.
    book.active = None;
    let source = anchor_source(state, bound.anchor).map_err(|_| invalid("Source missing"))?;
    if SourceRef::parse(&bound.address, Notation::A1, &book).ok().as_ref() != Some(&source) {
        return Err(invalid("Source missing"));
    }
    Ok(source)
}

pub(crate) fn bound_cell(state: &State, bound: &Bound) -> Result<CellKey, Refusal> {
    if let SourceRef::Cells { areas, .. } = bound_source(state, bound)?
        && let [area] = areas.as_slice()
        && area.0 == area.1
    {
        return cell(area.0);
    }
    Err(invalid("A page input must bind to one cell"))
}

pub(crate) fn source_areas(state: &State, source: &SourceRef) -> Result<Vec<GridArea>, Refusal> {
    let host = state.session.host();
    match source {
        SourceRef::Cells { sheet, areas } => {
            let id = checked_sheet(host, *sheet)?;
            let result = areas
                .iter()
                .copied()
                .map(grid_area)
                .collect::<Result<Vec<_>, _>>()?;
            if result.is_empty() || result.iter().any(|area| area.sheet() != id) {
                return Err(invalid("The source must specify areas on its worksheet"));
            }
            Ok(result)
        }
        SourceRef::Spill { anchor } => {
            let at = cell(*anchor)?;
            checked_sheet(host, Sheet::of(at.sheet()))?;
            Ok(vec![
                host.spill_area(at).unwrap_or_else(|| GridArea::cell(at)),
            ])
        }
        SourceRef::Sheet(sheet) => Ok(vec![GridArea::full_sheet(checked_sheet(host, *sheet)?)]),
        SourceRef::Table { .. } => Ok(crate::rows::table_area(state, source)?
            .into_iter()
            .collect()),
        SourceRef::Workbook => Ok(host
            .sheets()
            .iter()
            .map(|sheet| GridArea::full_sheet(sheet.id()))
            .collect()),
        _ => Err(unavailable(
            "dc-b67gh",
            "Resolving this source to worksheet cells",
        )),
    }
}

pub(crate) fn interest_areas(state: &State, interest: &Interest) -> Result<Vec<GridArea>, Refusal> {
    let host = state.session.host();
    match interest {
        Interest::Sheet(sheet) => Ok(host
            .used_area(checked_sheet(host, *sheet)?)
            .into_iter()
            .collect()),
        Interest::Selection => Ok(state.selection.areas.clone()),
        Interest::Anchors(ids) => {
            let areas = ids
                .iter()
                .map(|&id| source_areas(state, &anchor_source(state, id)?))
                .collect::<Result<Vec<_>, _>>()?;
            Ok(areas.into_iter().flatten().collect())
        }
        _ => {
            let list = match interest {
                Interest::Area(area) => std::slice::from_ref(area),
                Interest::Areas(areas) => areas.as_slice(),
                _ => unreachable!("handled above"),
            };
            list.iter()
                .map(|&area| {
                    let area = grid_area(area)?;
                    checked_sheet(host, Sheet::of(area.sheet()))?;
                    Ok(area)
                })
                .collect()
        }
    }
}

pub(crate) fn invalidated(
    state: &State,
    query: &WorkbookQuery,
    footprint: Option<GridArea>,
    prior: Option<Stamp>,
    selection_changed: bool,
    rtd_changed: bool,
) -> bool {
    let Some(prior) = prior else { return true };
    let now = state.stamp();
    if now.workbook != prior.workbook {
        return true;
    }
    let authored = prior.model != now.model;
    let recalculated = prior.calc != now.calc;
    let presentation = prior.view != now.view;
    let intersects = |areas: &[GridArea], pending: bool| {
        let mut touches = state
            .touches
            .iter()
            .filter(|touch| touch.revision > prior.model)
            .peekable();
        if touches
            .peek()
            .is_none_or(|touch| touch.revision.0 != prior.model.0 + 1)
        {
            return authored;
        }
        touches.any(|touch| {
            touch.areas.as_ref().is_none_or(|writes| {
                writes
                    .iter()
                    .any(|write| areas.iter().any(|area| area.intersection(*write).is_some()))
                    || (pending
                        && touch.pending.iter().any(|dirty| {
                            areas.iter().any(|area| area.intersection(*dirty).is_some())
                        }))
            })
        })
    };
    let scope_touched = || footprint.is_none_or(|area| intersects(&[area], false));
    let pending = || {
        state
            .touches
            .iter()
            .any(|touch| touch.revision > prior.model && !touch.pending.is_empty())
    };
    match query {
        WorkbookQuery::Rows { .. } | WorkbookQuery::Placement { .. } => {
            footprint.map_or(recalculated || authored, |area| {
                (authored && intersects(&[area], true))
                    || (recalculated && calculation_intersects(state, prior.calc, area))
            })
        }
        WorkbookQuery::NewSheetPlacement { .. } => authored,
        WorkbookQuery::Cells { interest } => {
            recalculated
                || (selection_changed && matches!(interest, Interest::Selection))
                || (presentation && matches!(interest, Interest::Anchors(_)))
                || (authored
                    && match interest_areas(state, interest) {
                        Ok(areas) if !matches!(interest, Interest::Sheet(_)) => {
                            intersects(&areas, true)
                        }
                        _ => true,
                    })
        }
        WorkbookQuery::Formula { at } => {
            authored
                && at
                    .cell()
                    .is_none_or(|at| intersects(&[GridArea::cell(at)], false))
        }
        WorkbookQuery::ExpressionTree { .. } | WorkbookQuery::TrialFor { .. } => {
            authored || recalculated || rtd_changed
        }
        WorkbookQuery::Geometry { .. } => authored || presentation,
        WorkbookQuery::PrintPlan { .. } => authored,
        WorkbookQuery::ViewPart => presentation || recalculated,
        WorkbookQuery::Notes { .. } => presentation,
        WorkbookQuery::Resolve { .. } => authored || selection_changed,
        WorkbookQuery::ResolveBound { .. } => authored || presentation,
        WorkbookQuery::Summary | WorkbookQuery::History => authored || presentation,
        WorkbookQuery::Census { .. } | WorkbookQuery::Issues { .. } => {
            recalculated || rtd_changed || (authored && (scope_touched() || pending()))
        }
        WorkbookQuery::Inventory { .. } => authored && intersects(&[], false),
        WorkbookQuery::DefinedName { .. } => authored,
        WorkbookQuery::Structure { scope } => {
            (authored && scope_touched())
                || (recalculated && matches!(scope, SourceRef::Spill { .. }))
        }
        WorkbookQuery::InputCandidates { .. } | WorkbookQuery::OutputCandidates { .. } => {
            authored || recalculated
        }
        WorkbookQuery::ChartImage { comparison, .. } => {
            authored || recalculated || rtd_changed || (comparison.is_some() && presentation)
        }
        WorkbookQuery::NumericComparison { .. } => authored || presentation || recalculated || rtd_changed,
        WorkbookQuery::Sources | WorkbookQuery::Contributions { .. } => {
            authored || recalculated || rtd_changed
        }
        WorkbookQuery::Deps { .. } => authored || recalculated,
        WorkbookQuery::CalcState => true,
        WorkbookQuery::CalcChanges => recalculated,
        WorkbookQuery::Rules { .. } => authored || recalculated,
    }
}

fn calculation_intersects(state: &State, prior: CalcGeneration, area: GridArea) -> bool {
    let mut next = prior.0 + 1;
    for touch in state
        .calc_touches
        .iter()
        .filter(|touch| touch.generation > prior)
    {
        if touch.generation.0 != next
            || touch.areas.as_ref().is_none_or(|areas| {
                areas
                    .iter()
                    .any(|&changed| area.intersection(changed).is_some())
            })
        {
            return true;
        }
        next += 1;
    }
    next != state.stamp().calc.0 + 1
}

pub(crate) fn footprint(state: &State, query: &WorkbookQuery) -> Option<GridArea> {
    match query {
        WorkbookQuery::Rows { source, .. } => crate::rows::footprint(state, source).ok(),
        WorkbookQuery::Placement { sheet, kind, .. } => {
            Some(crate::placement::footprint(*sheet, *kind))
        }
        WorkbookQuery::Census { scope }
        | WorkbookQuery::Issues { scope }
        | WorkbookQuery::InputCandidates { scope }
        | WorkbookQuery::OutputCandidates { scope }
        | WorkbookQuery::Structure { scope } => {
            let areas = source_areas(state, scope).ok()?;
            match areas.as_slice() {
                [area] => Some(*area),
                _ => None,
            }
        }
        _ => None,
    }
}

/// Admission and checkpoints for queries that scan beyond the returned window.
pub(crate) struct QueryWork {
    limit: Budget,
    pub(crate) counts: Budget,
    #[cfg(not(target_arch = "wasm32"))]
    started: std::time::Instant,
    #[cfg(target_arch = "wasm32")]
    started: f64,
    alternative: &'static str,
}

impl QueryWork {
    pub(crate) fn new(limit: Budget, alternative: &'static str) -> Self {
        Self::scan(
            Budget {
                cells: limit.cells.min(MAX_QUERY_CELLS),
                nodes: limit.nodes.min(MAX_QUERY_CELLS),
                ..limit
            },
            alternative,
        )
    }

    fn scan(limit: Budget, alternative: &'static str) -> Self {
        Self {
            limit: Budget {
                bytes: limit
                    .bytes
                    .min(calc_skin::tiles::MAX_PUBLICATION_BYTES as u64),
                ..limit
            },
            counts: Budget {
                cells: 0,
                nodes: 0,
                edges: 0,
                bytes: 0,
                millis: 0,
            },
            #[cfg(not(target_arch = "wasm32"))]
            started: std::time::Instant::now(),
            #[cfg(target_arch = "wasm32")]
            started: js_sys::Date::now(),
            alternative,
        }
    }

    pub(crate) fn check(&mut self) -> Result<(), Refusal> {
        #[cfg(not(target_arch = "wasm32"))]
        {
            self.counts.millis = self.started.elapsed().as_millis() as u64;
        }
        #[cfg(target_arch = "wasm32")]
        {
            self.counts.millis = (js_sys::Date::now() - self.started) as u64;
        }
        if self.counts.cells > self.limit.cells
            || self.counts.nodes > self.limit.nodes
            || self.counts.edges > self.limit.edges
            || self.counts.bytes > self.limit.bytes
            || self.limit.millis == 0
            || self.counts.millis > self.limit.millis
        {
            return Err(Refusal::OverBudget {
                counts: self.counts,
                alternatives: vec![self.alternative.into()],
            });
        }
        Ok(())
    }

    pub(crate) fn charge_bytes(&mut self, value: &impl Serialize) -> Result<(), Refusal> {
        self.counts.bytes += serde_json::to_vec(value)
            .map_err(|error| Refusal::Transport(error.to_string()))?
            .len() as u64;
        self.check()
    }

    pub(crate) fn work(&mut self, cells: u64, nodes: u64, edges: u64) -> Result<(), Refusal> {
        self.counts.cells = self.counts.cells.saturating_add(cells);
        self.counts.nodes = self.counts.nodes.saturating_add(nodes);
        self.counts.edges = self.counts.edges.saturating_add(edges);
        self.check()
    }
}

pub(crate) fn over_budget(cells: u64, bytes: u64) -> Refusal {
    Refusal::OverBudget {
        counts: Budget {
            cells,
            nodes: 0,
            edges: 0,
            bytes,
            millis: 0,
        },
        alternatives: vec!["Choose a smaller worksheet area".into()],
    }
}

pub(crate) fn area_size(areas: &[GridArea], budget: Budget) -> Result<(), Refusal> {
    let count = areas
        .iter()
        .fold(0u64, |n, area| n.saturating_add(area.cell_count()));
    if count > budget.cells.min(MAX_QUERY_CELLS) {
        return Err(over_budget(count, 0));
    }
    Ok(())
}

fn project_window(
    state: &State,
    interest: &Interest,
    budget: Budget,
) -> Result<CellWindow, Refusal> {
    let areas = interest_areas(state, interest)?;
    let host = state.session.host();
    let page = project_areas(host, &areas, budget)?;
    let cells = page
        .cells()
        .iter()
        .filter_map(|projected| {
            let visible = !matches!(projected.value(), ProjectedValue::Empty)
                || host.cell_style(projected.at()) != 0;
            visible.then(|| show_cell(state, projected))
        })
        .collect();
    Ok(CellWindow {
        areas: areas.into_iter().map(Area::of).collect(),
        cells,
    })
}

fn project_areas(
    host: &HostState,
    areas: &[GridArea],
    budget: Budget,
) -> Result<calc_host::ProjectionPage, Refusal> {
    area_size(areas, budget)?;
    let mut keys = BTreeSet::new();
    for area in areas {
        for row in area.first().row()..=area.last().row() {
            for column in area.first().column()..=area.last().column() {
                keys.insert(
                    CellKey::new(area.sheet(), row, column).expect("within the checked area"),
                );
            }
        }
    }
    calc_host::project(
        host,
        &ProjectionRequest {
            cells: keys.into_iter().collect(),
        },
    )
    .map_err(|error| invalid(error.to_string()))
}

fn trial_for(state: &State, at: Key, budget: Budget) -> Result<Trial, Refusal> {
    let host = state.session.host();
    let cell = cell(at)?;
    checked_sheet(host, Sheet::of(cell.sheet()))?;
    let input = host.model().cell(cell);
    let mut trial = Trial::default();
    if let Some(profile) = host.options().locale {
        trial.culture = profile.id.stable_name().into();
    }
    if let Some(format) = host.number_format(cell) {
        trial.number_format = format.into();
    }
    let precedents = if matches!(input, Some(calc_doc_model::CellInput::Formula(_))) {
        trial.formula = calc_formula_kernel::display::formula(&entry_text(input));
        let areas = host.precedents(cell)
            .ok_or_else(|| invalid("Calculate the workbook to capture this formula's inputs."))?;
        if areas.iter().any(|area| area.contains(cell)) {
            return Err(unavailable("dc-s2xup", "Explorer trial with a circular caller"));
        }
        if areas.len() as u64 > budget.edges {
            return Err(over_budget(areas.len() as u64, 0));
        }
        let page = project_areas(host, &areas, budget)?;
        page.cells()
            .iter()
            .map(|cell| {
                let (value, provenance) = shown_value(state, cell.value());
                SnapshotCell {
                    at: Key::of(cell.at()),
                    shown: Shown {
                        value: value.as_ref().map_or(RawValue::Blank, raw),
                        provenance,
                    },
                }
            })
            .collect()
    } else {
        Vec::new()
    };
    trial.caller = Some(CallerSnapshot {
        at,
        stamp: state.stamp(),
        date_1904: host.declared_date1904(),
        precedents,
        sheets: host.sheets().iter().filter(|sheet| sheet.kind() == SheetKind::Worksheet)
            .map(|sheet| CallerSheet {
                sheet: Sheet::of(sheet.id()),
                name: sheet.name().text().to_string_lossy(),
            })
            .collect(),
    });
    Ok(trial)
}

pub(crate) fn shown_value(
    state: &State,
    value: &ProjectedValue,
) -> (Option<CalcValue>, Provenance) {
    let host = state.session.host();
    let stamp = state.stamp();
    match value {
        ProjectedValue::Empty => (None, Provenance::Empty),
        ProjectedValue::Literal(value) => (Some(value.clone()), Provenance::Literal),
        ProjectedValue::FileCached(cache) => (Some(cache.value().to_value()), Provenance::AsSaved),
        ProjectedValue::Calculated { value, calculation } => {
            let provenance = if calculation.held_by_circular_rule() {
                Provenance::Stale {
                    since: stamp.model,
                    cause: StaleCause::CircularHold,
                }
            } else if calculation.reads_external_links() {
                Provenance::External {
                    source: "Workbook link cache".into(),
                    observed_at: None,
                }
            } else {
                Provenance::Calculated { calc: stamp.calc }
            };
            (Some(value.clone()), provenance)
        }
        ProjectedValue::NotModeled { refusal, .. } => (
            None,
            Provenance::Refused {
                owner: "dc-b67gh".into(),
                reason: format!("{refusal:?}"),
            },
        ),
        ProjectedValue::Stale(stale) => {
            let cause = if stale.cause() == calc_host::StaleCause::RecalcFailed {
                StaleCause::CalcFailed
            } else if host.calc_mode() == calc_host::CalcMode::Manual {
                StaleCause::ManualMode
            } else {
                StaleCause::Calculating
            };
            (
                stale
                    .last_value()
                    .cloned()
                    .or_else(|| stale.file_cache().map(|cache| cache.value().to_value())),
                Provenance::Stale {
                    since: stamp.model,
                    cause,
                },
            )
        }
    }
}

pub(crate) fn show_cell(state: &State, projected: &ProjectedCell) -> WorkbookCell {
    let host = state.session.host();
    let at = projected.at();
    let (value, provenance) = shown_value(state, projected.value());
    let input = entry_text(projected.input());
    let input = if matches!(
        projected.input(),
        Some(calc_doc_model::CellInput::Formula(_))
    ) {
        calc_formula_kernel::display::formula(&input)
    } else {
        input
    };
    let locale = host.options().locale.as_ref();
    let text = value.as_ref().map_or_else(String::new, |value| {
        if let Some(alt) = value.image_value() {
            return alt.alt_text.clone().unwrap_or_default();
        }
        let Some((profile, code)) = locale.zip(host.number_format(at)) else {
            return calc_formula_kernel::result::shown(value, locale);
        };
        let system = if host.declared_date1904() {
            calc_func::locale::DateSystem::System1904
        } else {
            calc_func::locale::DateSystem::System1900
        };
        match calc_func::cell_text_localized(profile, system, value, code) {
            Ok(text) => text,
            Err(calc_func::CellFormatError::Unshowable) => "#####".into(),
            Err(calc_func::CellFormatError::NotModelled) => {
                calc_formula_kernel::result::shown(value, locale)
            }
        }
    });
    let spill = projected.spill().map(|spill| match spill {
        calc_host::ProjectedSpill::Filled(area) => SpillExtent::Filled(Area::of(area)),
        calc_host::ProjectedSpill::Blocked(area) => SpillExtent::Blocked(Area::of(area)),
    });
    WorkbookCell {
        at: Key::of(at),
        input,
        shown: Shown {
            value: value.as_ref().map_or(RawValue::Blank, raw),
            provenance,
        },
        text,
        spill,
        style: host.cell_style(at),
    }
}

fn address_book(state: &State) -> AddressBook {
    let host = state.session.host();
    AddressBook {
        sheets: host
            .sheets()
            .iter()
            .map(|sheet| (Sheet::of(sheet.id()), sheet.name().text().to_string_lossy()))
            .collect(),
        names: host
            .defined_names()
            .iter()
            .map(|name| {
                let scope = match name.scope {
                    calc_doc_model::NameScope::Workbook => NameScope::Workbook,
                    calc_doc_model::NameScope::Sheet(sheet) => NameScope::Sheet(Sheet::of(sheet)),
                };
                (scope, name.name.clone())
            })
            .collect(),
        tables: host
            .model()
            .definitions()
            .tables()
            .iter()
            .map(|table| table.display_name.clone())
            .collect(),
        active: state.selection.active.map(Key::of),
    }
}

pub(crate) fn answer(state: &State, query: &WorkbookQuery, budget: Budget) -> AnswerResult {
    let host = state.session.host();
    let stamp = state.stamp();
    let mut coverage = Coverage::Complete;
    let alternative = match query {
        WorkbookQuery::Census { .. }
        | WorkbookQuery::Issues { .. }
        | WorkbookQuery::Structure { .. }
        | WorkbookQuery::OutputCandidates { .. } => Some("Choose a smaller worksheet area"),
        WorkbookQuery::InputCandidates { .. } => Some("Choose a smaller worksheet area or open Forms maker"),
        WorkbookQuery::ChartImage { .. } => Some("Choose a chart with fewer points"),
        WorkbookQuery::Deps { .. } => Some("Choose a smaller area or dependency depth"),
        WorkbookQuery::Inventory { .. } => Some("Choose fewer workbook object kinds"),
        WorkbookQuery::Notes { .. } => Some("Choose one anchor"),
        WorkbookQuery::Sources => Some("Increase the source inventory budget"),
        WorkbookQuery::Contributions { .. } => Some("Increase the contribution scan budget"),
        WorkbookQuery::TrialFor { .. } => Some("Choose a formula that reads fewer cells"),
        _ => None,
    };
    let mut meter = QueryWork::scan(
        budget,
        alternative.unwrap_or("Choose a smaller worksheet area"),
    );
    if alternative.is_some() {
        meter.check()?;
    }
    let body = match query {
        WorkbookQuery::Summary => query::Summary::encode(&WorkbookSummary {
            name: host
                .model()
                .definitions()
                .workbook_name()
                .unwrap_or("Book1")
                .to_owned(),
            sheets: host
                .sheets()
                .iter()
                .map(|sheet| SheetSummary {
                    sheet: Sheet::of(sheet.id()),
                    name: sheet.name().text().to_string_lossy(),
                    visible: sheet.visibility() == SheetVisibility::Visible,
                    worksheet: sheet.kind() == SheetKind::Worksheet,
                })
                .collect(),
            edited: stamp.model != state.opened_at.model,
            layout_edited: stamp.view != state.opened_at.view,
            date1904: host.declared_date1904(),
            macros: host.model().definitions().macro_enabled(),
            capabilities: capabilities(state),
        })?,
        WorkbookQuery::Census { scope } => {
            query::CensusOf::encode(&survey::census(state, scope, &mut meter)?)?
        }
        WorkbookQuery::Inventory { kinds } => {
            query::Inventory::encode(&inventory::inventory(host, kinds, &mut meter)?)?
        }
        WorkbookQuery::DefinedName { scope, name } => {
            meter.work(0, 1, 0)?;
            query::DefinedName::encode(&crate::names::facts(host, scope, name)?)?
        }
        WorkbookQuery::Structure { scope } => query::StructureOf::encode(&Evidence::Derived {
            from: Derivation::Structure,
            at: stamp,
            body: structure::structure(state, scope, &mut meter)?,
        })?,
        WorkbookQuery::InputCandidates { scope } => {
            query::InputCandidates::encode(&structure::inputs(state, scope, &mut meter)?)?
        }
        WorkbookQuery::OutputCandidates { scope } => {
            query::OutputCandidates::encode(&structure::outputs(state, scope, &mut meter)?)?
        }
        WorkbookQuery::ChartImage { object, comparison } => {
            if let Some(bound) = comparison && &bound_source(state, bound)? != object {
                return Err(invalid("Source missing"));
            }
            query::ChartImageOf::encode(&chart_image(state, object, comparison.as_ref(), &mut meter)?)?
        }
        WorkbookQuery::NumericComparison { bound } => {
            meter.work(1, 1, 0)?;
            query::NumericComparisonOf::encode(&crate::comparison::scalar(state, bound)?)?
        }
        WorkbookQuery::Sources => {
            query::SourceInventory::encode(&inventory::sources(host, &mut meter)?)?
        }
        WorkbookQuery::Deps {
            of,
            dir,
            depth,
            group,
        } => {
            let body = dependencies::deps(state, of, *dir, *depth, *group, &mut meter)?;
            if !body.unbound.is_empty() {
                coverage = Coverage::Partial {
                    done: body.edges.len() as u64,
                    total: None,
                    why: PartialWhy::Pending,
                };
            }
            query::Deps::encode(&Evidence::Derived {
                from: Derivation::Dependencies,
                at: stamp,
                body,
            })?
        }
        WorkbookQuery::Issues { scope } => query::IssuesIn::encode(&Evidence::Derived {
            from: Derivation::Dependencies,
            at: stamp,
            body: issues::issues(state, scope, &mut meter)?,
        })?,
        WorkbookQuery::Contributions { at } => {
            query::ContributionsAt::encode(&Evidence::Derived {
                from: Derivation::Contributions,
                at: stamp,
                body: contributions::contributions(state, *at, &mut meter)?,
            })?
        }
        WorkbookQuery::Cells { interest } => {
            query::Cells::encode(&project_window(state, interest, budget)?)?
        }
        WorkbookQuery::Rows {
            source,
            filter,
            sort,
            window,
        } => {
            let (page, completeness) =
                crate::rows::read(state, source, filter, sort, *window, budget)?;
            coverage = completeness;
            query::Rows::encode(&page)?
        }
        WorkbookQuery::Placement {
            sheet,
            kind,
            shape,
            reserved,
        } => query::Placement::encode(&crate::placement::find(
            state, crate::placement::BackingSheet::Existing(*sheet), *kind, *shape, reserved, budget,
        )?)?,
        WorkbookQuery::NewSheetPlacement { name, kind, shape } => query::NewSheetPlacement::encode(&crate::placement::find(
            state, crate::placement::BackingSheet::New(name), *kind, *shape, &[], budget,
        )?)?,
        WorkbookQuery::Geometry { sheet } => {
            let id = checked_sheet(host, *sheet)?;
            geometry(host, id)?
        }
        WorkbookQuery::Formula { at } => {
            let at = cell(*at)?;
            checked_sheet(host, Sheet::of(at.sheet()))?;
            let input = host.model().cell(at);
            let formula = matches!(input, Some(calc_doc_model::CellInput::Formula(_)));
            let text = entry_text(input);
            let precedents = host.precedents(at).unwrap_or_default();
            if precedents.len() as u64 > budget.edges {
                return Err(over_budget(precedents.len() as u64, 0));
            }
            let loaded = host.loaded();
            let as_saved = loaded.sheets().iter().find(|sheet| sheet.id() == at.sheet())
                .and_then(|sheet| sheet.formula(at.row(), at.column()))
                .and_then(|formula| formula.cache())
                .map(|cache| query::AsSavedFormula {
                    entry: calc_formula_kernel::display::formula(&entry_text(loaded.model().cell(at))),
                    value: raw(&cache.value().to_value()),
                });
            let authored = formula.then(|| text.clone());
            query::Formula::encode(&query::FormulaFacts {
                input: if formula {
                    calc_formula_kernel::display::formula(&text)
                } else {
                    text
                },
                formula,
                precedents: precedents.into_iter().map(Area::of).collect(),
                authored,
                as_saved,
            })?
        }
        WorkbookQuery::ExpressionTree { at } => {
            let at = cell(*at)?;
            let evidence = expression_tree(host, at, stamp, &mut meter)?;
            query::ExpressionTree::encode(&evidence)?
        }
        WorkbookQuery::TrialFor { at } => query::TrialFor::encode(&trial_for(state, *at, budget)?)?,
        WorkbookQuery::Rules { at } => {
            let at = cell(*at)?;
            checked_sheet(host, Sheet::of(at.sheet()))?;
            let refusal = if state.read_only {
                Some(Refusal::ReadOnly)
            } else {
                batch::rules(&state.session, at).err()
            };
            let validation = host.validation(at).map(|rule| format!("{rule:?}"));
            query::Rules::encode(&query::CellRules {
                refusal,
                validation,
            })?
        }
        WorkbookQuery::CalcState => query::CalcState::encode(&query::CalcStatus {
            mode: match host.calc_mode() {
                calc_host::CalcMode::Automatic => query::CalcMode::Automatic,
                calc_host::CalcMode::AutomaticExceptTables => {
                    query::CalcMode::AutomaticExceptTables
                }
                calc_host::CalcMode::Manual => query::CalcMode::Manual,
            },
            // The host reports Excel's CalculationState: 0 is xlDone.
            activity: if host.calculation_state() == 0 {
                query::CalcActivity::Done
            } else {
                query::CalcActivity::Pending
            },
            circular: host.circular_reported().map(Key::of),
        })?,
        WorkbookQuery::CalcChanges => {
            let evidence = match &state.calculation_changes {
                Some(changes) => {
                    coverage = changes.coverage.clone();
                    Evidence::Observed {
                        observer: "Formula result publication".into(),
                        at: changes.stamp,
                        body: changes.values.clone(),
                    }
                }
                None => Evidence::Unavailable {
                    reason: "No observed formula result publication".into(),
                },
            };
            query::CalcChanges::encode(&evidence)?
        }
        WorkbookQuery::ViewPart => {
            let part = host.model().view_part();
            let anchors = part
                .anchors()
                .map(|anchor| {
                    let source = anchor_source(state, anchor.id).ok();
                    let area = source
                        .as_ref()
                        .and_then(|source| source_areas(state, source).ok())
                        .and_then(|areas| areas.first().copied())
                        .map(Area::of);
                    query::AnchorFacts {
                        id: anchor.id,
                        source,
                        area,
                    }
                })
                .collect();
            let mut documents = Vec::new();
            for doc in part.documents() {
                meter.check()?;
                documents.push(query::ViewDocumentFacts {
                    id: doc.id,
                    view: ViewId(doc.view.clone()),
                    sheet: doc
                        .sheet
                        .and_then(|key| host.model().sheet_by_key(key))
                        .map(|sheet| Sheet::of(sheet.id())),
                    schema: doc.schema,
                    body: serde_json::from_str(&doc.body)
                        .map_err(|error| invalid(format!("Invalid view declaration: {error}")))?,
                });
            }
            query::ViewPartOf::encode(&query::ViewPartFacts {
                opaque: part.is_opaque(),
                preferred: part.preferred().map(|name| ViewId(name.into())),
                anchors,
                documents,
            })?
        }
        WorkbookQuery::Notes { anchor } => {
            let part = host.model().view_part();
            if part.is_opaque() {
                return Err(unavailable("dc-b67gh", "Notes in an unreadable view part"));
            }
            let mut notes = Vec::new();
            for note in part.notes() {
                meter.check()?;
                if anchor.is_none_or(|anchor| anchor == note.anchor) {
                    meter.work(0, 1, 0)?;
                    notes.push(AnchoredNote {
                        id: note.id,
                        anchor: note.anchor,
                        text: note.text.clone(),
                        source: anchor_source(state, note.anchor).ok(),
                    });
                }
            }
            query::NotesOf::encode(&notes)?
        }
        WorkbookQuery::History => {
            let (undo, redo) = state.session.history_depth();
            let (undo_entries, redo_entries) = state.session.skin_history();
            query::History::encode(&query::HistoryList {
                undo: undo_entries,
                redo: redo_entries,
                undo_depth: undo as u64,
                redo_depth: redo as u64,
            })?
        }
        WorkbookQuery::Resolve { address } => {
            let source = SourceRef::parse(address, Notation::A1, &address_book(state))
                .map_err(|error| invalid(format!("Cannot resolve the address ({error:?})")))?;
            query::Resolve::encode(&source)?
        }
        WorkbookQuery::ResolveBound { bound } => query::ResolveBound::encode(&bound_source(state, bound)?)?,
        WorkbookQuery::PrintPlan { source } => {
            let areas = source_areas(state, source)?;
            let sheet = areas
                .first()
                .map(|area| area.sheet())
                .ok_or_else(|| invalid("Nothing to print"))?;
            if !matches!(source, SourceRef::Sheet(_)) {
                return Err(unavailable(
                    "dc-s2xup",
                    "Print plan for a selection or workbook",
                ));
            }
            let geometry = calc_host::print_plan::print_geometry(host, sheet);
            let plan = calc_host::print_plan::print_plan(host, sheet, &geometry, (0, 0))
                .map_err(|reason| Refusal::Excel { reason })?;
            query::PrintPlanOf::encode(&plan)?
        }
    };
    let bytes = serde_json::to_vec(&body)
        .map_err(|error| Refusal::Transport(error.to_string()))?
        .len() as u64;
    if alternative.is_some() {
        meter.counts.bytes = bytes;
        meter.check()?;
    } else if bytes > budget.bytes {
        return Err(over_budget(0, bytes));
    }
    Ok(Answer {
        stamp,
        coverage,
        body,
    })
}

fn geometry(host: &HostState, id: calc_host::value::SheetId) -> Result<Value, Refusal> {
    let geometry = host.sheet_geometry(id);
    let size = geometry.as_ref();
    let view = host.sheet_view(id);
    let definitions = host.model().definitions();
    let panes = definitions.frozen_panes(id);
    query::Geometry::encode(&WorksheetGeometry {
        rows: size
            .into_iter()
            .flat_map(|g| g.rows.iter())
            .map(|(&row, format)| (row, format.height, format.hidden))
            .collect(),
        columns: size
            .into_iter()
            .flat_map(|g| &g.columns)
            .map(|span| (span.first, span.last, span.width, span.hidden))
            .collect(),
        default_row_height: size
            .and_then(|g| g.default_row_height)
            .unwrap_or_else(|| host.default_font_row_points()),
        default_column_width: size.and_then(|g| g.default_column_width).unwrap_or(8.43),
        default_digit_px: host.default_digit_px(),
        merges: host.merged_areas(id).into_iter().map(Area::of).collect(),
        frozen_rows: panes.map_or(0, |p| p.rows()),
        frozen_columns: panes.map_or(0, |p| p.columns()),
        zoom: view.as_ref().and_then(|v| v.zoom).unwrap_or(100),
        show_grid_lines: view.and_then(|v| v.show_grid_lines),
        auto_filter_areas: definitions.auto_filter(id).into_iter().chain(
            definitions.tables().iter().filter(|table| table.sheet == id)
                .filter_map(|table| table.filter.as_ref())
        ).map(|filter| Area::of(filter.area)).collect(),
    })
}

fn expression_tree(
    host: &HostState,
    at: CellKey,
    stamp: Stamp,
    meter: &mut QueryWork,
) -> Result<Evidence<calc_skin::parts::Trace>, Refusal> {
    if host.calculation_state() != 0 {
        return Ok(Evidence::Unavailable {
            reason: "The calculation is still pending.".into(),
        });
    }
    let Some(input @ calc_doc_model::CellInput::Formula(_)) = host.model().cell(at) else {
        return Ok(Evidence::Unavailable {
            reason: "This cell has no formula.".into(),
        });
    };
    struct Evaluator<'a> {
        host: &'a HostState,
        at: CellKey,
        meter: &'a mut QueryWork,
    }
    impl calc_formula_kernel::parts::PartEvaluator for Evaluator<'_> {
        fn calculate(&mut self, text: &str) -> Result<CalcValue, String> {
            self.meter
                .check()
                .map_err(|_| "Expression inspection exceeded its budget.".to_string())?;
            self.host
                .evaluate_formula_text(text.strip_prefix('=').unwrap_or(text), self.at)
        }

        fn profile(&self) -> Option<calc_host::FormatProfile> {
            self.host.options().locale.clone()
        }
    }
    let mut evaluator = Evaluator { host, at, meter };
    let formula = calc_formula_kernel::display::formula(&entry_text(Some(input)));
    let body = calc_formula_kernel::parts::trace(&mut evaluator, &formula);
    meter.check()?;
    Ok(Evidence::Derived {
        from: Derivation::ExpressionStructure,
        at: stamp,
        body,
    })
}
