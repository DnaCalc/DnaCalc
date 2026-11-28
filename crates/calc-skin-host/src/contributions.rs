use crate::{Refusal, State, cell, invalid, query::QueryWork, query::raw, unavailable};
use calc_func::{ConditionalObserver, CriterionVerdict, FunctionId};
use calc_host::value::{CellKey, GridArea};
use calc_skin::grid::{Area, Key};
use calc_skin::{ContributionCriterion, Contributions};

pub(crate) fn contributions(
    state: &State,
    at: Key,
    meter: &mut QueryWork,
) -> Result<Contributions, Refusal> {
    let host = state.session.host();
    let anchor = cell(at)?;
    if host.model().sheet_by_id(anchor.sheet()).is_none() {
        return Err(invalid("The worksheet is unavailable"));
    }
    let mut observer = Collector {
        meter,
        observed: None,
        refusal: None,
    };
    let evaluated = host.observe_conditional(anchor, &mut observer);
    if let Some(refusal) = observer.refusal {
        return Err(refusal);
    }
    let observation = observer.observed.ok_or_else(|| {
        unavailable("dc-b67gh", "Contribution analysis needs a bound COUNTIFS, SUMIFS, AVERAGEIFS, FILTER, or XLOOKUP calculation")
    })?;
    let result = evaluated.map_err(|error| Refusal::NotModelled {
        owner: "dc-b67gh".into(),
        what: error.to_string(),
    })?;
    Ok(Contributions {
        formula: at,
        function: observation.function,
        source: Area::of(observation.source),
        inspected: observation.inspected,
        matching_count: observation.matching_count,
        matching: observation.matching,
        selected: observation.selected,
        undetermined: observation.undetermined,
        criteria: observation.criteria,
        result: raw(&result),
    })
}

struct Collector<'a> {
    meter: &'a mut QueryWork,
    observed: Option<Observation>,
    refusal: Option<Refusal>,
}

struct Observation {
    function: String,
    source: GridArea,
    inspected: u64,
    matching_count: u64,
    matching: Vec<Area>,
    selected: Option<Area>,
    undetermined: u64,
    criteria: Vec<ContributionCriterion>,
}

impl ConditionalObserver for Collector<'_> {
    fn begin(&mut self, id: FunctionId, ranges: &[GridArea], target: Option<GridArea>) -> bool {
        if self.observed.is_some() {
            self.refusal = Some(unavailable(
                "dc-b67gh",
                "Inspecting multiple conditional invocations in one formula",
            ));
            return false;
        }
        let source = target.unwrap_or(ranges[0]);
        let cells = ranges
            .iter()
            .copied()
            .chain(target)
            .map(GridArea::cell_count)
            .sum();
        if let Err(refusal) = self.meter.work(cells, ranges.len() as u64, 0) {
            self.refusal = Some(refusal);
            return false;
        }
        let function = calc_func::catalogue()
            .resolve(id)
            .expect("kernel entry")
            .spelling()
            .to_owned();
        let label = match function.as_str() {
            "FILTER" => Some("Include mask"),
            "XLOOKUP" => Some("Lookup criterion"),
            _ => None,
        };
        self.observed = Some(Observation {
            function,
            source,
            inspected: source.cell_count(),
            matching_count: 0,
            matching: Vec::new(),
            selected: None,
            undetermined: 0,
            criteria: ranges
                .iter()
                .copied()
                .map(|range| ContributionCriterion {
                    range: Area::of(range),
                    label: label.map(str::to_owned),
                    matching: 0,
                    failing_only_this: Vec::new(),
                })
                .collect(),
        });
        true
    }

    fn observe(&mut self, offset: u64, verdicts: &[CriterionVerdict]) -> bool {
        use CriterionVerdict::{No, Unknown, Yes};

        if let Err(refusal) = self.meter.work(0, 0, 0) {
            self.refusal = Some(refusal);
            return false;
        }
        let observed = self.observed.as_mut().expect("observation admitted");
        let mut failures = 0;
        let mut unknown = false;
        let mut failed = 0;
        for (index, (&verdict, criterion)) in
            verdicts.iter().zip(&mut observed.criteria).enumerate()
        {
            match verdict {
                Yes => criterion.matching += 1,
                No => {
                    failures += 1;
                    failed = index;
                }
                Unknown => unknown = true,
            }
        }
        if unknown {
            observed.undetermined += u64::from(failures == 0);
        } else if failures <= 1 {
            let at = position(observed.source, offset);
            if failures == 0 {
                observed.matching_count += 1;
                add_position(&mut observed.matching, Key::of(at));
            } else {
                add_position(
                    &mut observed.criteria[failed].failing_only_this,
                    Key::of(at),
                );
            }
        }
        true
    }

    fn selected(&mut self, offset: u64) {
        let observed = self.observed.as_mut().expect("observation admitted");
        let at = Key::of(position(observed.source, offset));
        observed.selected = Some(Area(at, at));
    }
}

fn position(source: GridArea, offset: u64) -> CellKey {
    let top = source.first();
    let width = u64::from(source.column_count());
    CellKey::new(
        top.sheet(),
        top.row() + (offset / width) as u32,
        top.column() + (offset % width) as u32,
    )
    .expect("within the admitted range")
}

fn add_position(spans: &mut Vec<Area>, at: Key) {
    if let Some(last) = spans.last_mut()
        && ((last.0.2 == at.2 && last.1.2 == at.2 && last.1.3 + 1 == at.3)
            || (last.0.3 == at.3 && last.1.3 == at.3 && last.1.2 + 1 == at.2))
    {
        last.1 = at;
    } else {
        spans.push(Area(at, at));
    }
}
