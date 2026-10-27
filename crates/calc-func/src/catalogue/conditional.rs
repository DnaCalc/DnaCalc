//! The conditional aggregate family fragment: COUNTIF, COUNTIFS, SUMIF, SUMIFS, AVERAGEIF,
//! AVERAGEIFS, MAXIFS and MINIFS, their criteria language and their kernels.
//!
//! A call pairs criteria ranges with criteria. Each criteria range is a reference of one area; the
//! cells of all of a call's ranges are aligned by their row-major offset inside their own area, so
//! ranges of one shape need not share a position. SUMIF and AVERAGEIF shape their third argument
//! from its top-left cell to the criteria range's size; the *IFS functions require every range to
//! have the target range's shape (COUNTIFS: the first criteria range's) and return #VALUE!
//! otherwise, decided before any cell or criterion is read. A range argument that is not a
//! reference ends the call in ReferenceRequired, and a reference of several areas is #VALUE!,
//! before any criterion is read (dc-b1l, measured in Excel 16.0.20430 on 2026-10-25: a union at
//! any range of each function, with a text or an error criterion). A shaped third argument may
//! extend past the reference the formula gives: the call reads the cells past it through the
//! cell reader, and the engine files every area an evaluation read beyond its placed references
//! as a dynamic prerequisite (L2), so a change there recalculates the formula. Past the last row
//! of the sheet it holds no cells (Excel: SUMIF(B3:B6,"a",A1048575) adds A1048576 alone); past
//! the last column it is not modelled, since Excel's alignment of the cut rows is not measured.
//!
//! A criterion is read once, before any range. A reference criterion is dereferenced through the
//! cell reader, a blank cell reading as Empty. A number, a logical, an error, or Empty or an
//! omitted argument (both 0) asks for equality. Text longer than 255 characters, operator
//! included, is #VALUE!. Otherwise text may start with an operator (=, <>, <, >, <=, >=; none is
//! =) followed by its operand: nothing (`""` matches blank cells and empty text, `=` blank cells
//! only, `<>` every cell but a blank one, and <, <=, > or >= no cell), an error literal, a
//! pattern when = or <> is followed by text holding *, ? or ~ (* any run, ? one character, ~
//! makes a following *, ? or ~ literal and is itself literal before any other character or at
//! the end), else a number when the operand reads as one by worksheet number coercion, a logical
//! when it spells TRUE or FALSE, and text otherwise (a label such as 2016-17 Winter, which
//! coercion finds is no number, is text). An array, an operand that coercion refuses (number text
//! by locale rules) and an error literal Excel added after #N/A are not modelled.
//!
//! A cell meets a criterion by kind. A number criterion compares numbers; text meets it under =
//! when the text reads as that number, never under <, <=, > or >=, and always under <>. A text
//! criterion compares text; a logical criterion logicals, FALSE below TRUE; an error criterion
//! errors, in the order #NULL!, #DIV/0!, #VALUE!, #REF!, #NAME?, #NUM!, #N/A; a cell of another
//! kind does not meet it. Apart from text under a number criterion, <> holds exactly where =
//! does not, blank cells included. Numbers compare by the comparison operators' rule and text by
//! their text rule, so where that rule refuses (text holding a character beyond its collation's
//! repertoire, and nearly equal numbers in the bands where Excel's rounding departs from the exact
//! one) the cell's verdict is unknown; patterns match text ignoring case character by character
//! (the simple lower-case mapping, so ß matches no ss) and are unknown on text holding a
//! character beyond the BMP, except a pattern of * alone, which matches every text. The verdict is also unknown for a text longer
//! than 255 characters against a text criterion or a pattern, for text that coercion refuses
//! against = a number, for text spelling TRUE or FALSE against a logical criterion, for text
//! spelling the criterion's error literal (or any, against an ordering), and for an error added
//! after #N/A ordered against an error literal. A row's verdict is no when any criterion rules it
//! out, else unknown when any verdict is unknown. A call refuses as not modelled when an unknown
//! row decides its result: any such row for a count, and for the others one whose target cell
//! holds a number or an error.
//!
//! The kernels read every range through the services' cell reader, which visits the cells that
//! hold a value (a blank cell it visits as Empty is blank), so a call's work and memory follow the
//! cells the reader visits and not the ranges' size: a criteria range keeps one verdict per
//! visited cell, a blank cell takes the criterion's verdict on a blank, and a count adds the
//! blank rows of all ranges at once. The ranges are read in argument order, the target last
//! unless it is a criteria range already read. In the target, numbers are used and text,
//! logicals and blank cells are skipped; the first error in a matched row, in row-major order, is
//! the result. Comparing two different numbers and reading number text are arithmetic sites, as
//! they are for the comparison operators and for coercion. The sums and averages are a
//! sequential left fold from 0 in row-major order, each operation an arithmetic site; every site
//! rounds under the call's numeric mode, so under Exact, which has no implementation, a call that
//! reaches one is refused with ExactUnavailable. The extremes choose without rounding.

use super::aggregate::{read_reference, reader};
use super::comparison::{self, Want};
use super::{ArgumentDemand, CatalogueEntry, Family, FunctionId, Kernel, Notation, Parameter};
use crate::call::{EagerKernel, EvalServices, FunctionFault};
use crate::coerce;
use crate::kernel::{Exit, Site, Step};
use crate::value::{
    CalcValue, CallableArityShape, CellKey, CoreValue, ExcelText, GridArea, MAX_COLUMNS, MAX_ROWS,
    ReferenceLike, WorksheetErrorCode,
};
use std::cmp::Ordering;

const CODE: u16 = 7;

const COUNTIF: FunctionId = FunctionId::new(CODE, 1);
const COUNTIFS: FunctionId = FunctionId::new(CODE, 2);
const SUMIF: FunctionId = FunctionId::new(CODE, 3);
const SUMIFS: FunctionId = FunctionId::new(CODE, 4);
const AVERAGEIF: FunctionId = FunctionId::new(CODE, 5);
const AVERAGEIFS: FunctionId = FunctionId::new(CODE, 6);
const MAXIFS: FunctionId = FunctionId::new(CODE, 7);
const MINIFS: FunctionId = FunctionId::new(CODE, 8);

/// The longest criterion text, operator and escapes included, in UTF-16 code units: a longer one
/// is #VALUE!. Also the longest text a text criterion or a pattern compares: Excel's pages for
/// SUMIF and COUNTIF say they return incorrect results beyond 255 characters.
const LONGEST_TEXT: usize = 255;

const RANGE_CRITERIA: &[Parameter] = &[
    Parameter::reference("range"),
    Parameter::value_or_reference("criteria"),
];
const SUM_RANGE: &[Parameter] = &[
    Parameter::reference("range"),
    Parameter::value_or_reference("criteria"),
    Parameter::reference("sum_range"),
];
const AVERAGE_RANGE: &[Parameter] = &[
    Parameter::reference("range"),
    Parameter::value_or_reference("criteria"),
    Parameter::reference("average_range"),
];
// The catalogue repeats only the last named parameter, so arguments past these names take its
// class, ValueOrReference: a criteria range there reaches the kernel, which requires a reference.
const COUNT_PAIRS: &[Parameter] = &[
    Parameter::reference("criteria_range1"),
    Parameter::value_or_reference("criteria1"),
    Parameter::reference("criteria_range2"),
    Parameter::value_or_reference("criteria2"),
];
const SUM_PAIRS: &[Parameter] = &[
    Parameter::reference("sum_range"),
    Parameter::reference("criteria_range1"),
    Parameter::value_or_reference("criteria1"),
    Parameter::reference("criteria_range2"),
    Parameter::value_or_reference("criteria2"),
];
const AVERAGE_PAIRS: &[Parameter] = &[
    Parameter::reference("average_range"),
    Parameter::reference("criteria_range1"),
    Parameter::value_or_reference("criteria1"),
    Parameter::reference("criteria_range2"),
    Parameter::value_or_reference("criteria2"),
];
const MAX_PAIRS: &[Parameter] = &[
    Parameter::reference("max_range"),
    Parameter::reference("criteria_range1"),
    Parameter::value_or_reference("criteria1"),
    Parameter::reference("criteria_range2"),
    Parameter::value_or_reference("criteria2"),
];
const MIN_PAIRS: &[Parameter] = &[
    Parameter::reference("min_range"),
    Parameter::reference("criteria_range1"),
    Parameter::value_or_reference("criteria1"),
    Parameter::reference("criteria_range2"),
    Parameter::value_or_reference("criteria2"),
];

/// An eager named call taking `min` to `max` arguments.
const fn conditional(
    id: FunctionId,
    spelling: &'static str,
    (min, max): (usize, usize),
    parameters: &'static [Parameter],
    kernel: EagerKernel,
) -> CatalogueEntry {
    CatalogueEntry {
        id,
        spelling,
        notation: Notation::Call,
        arity: CallableArityShape::range(min, max),
        parameters,
        demand: ArgumentDemand::Eager,
        kernel: Kernel::Eager(kernel),
    }
}

pub(super) const FAMILY: Family = Family {
    code: CODE,
    entries: &[
        conditional(COUNTIF, "COUNTIF", (2, 2), RANGE_CRITERIA, countif),
        conditional(COUNTIFS, "COUNTIFS", (2, 254), COUNT_PAIRS, countifs),
        conditional(SUMIF, "SUMIF", (2, 3), SUM_RANGE, sumif),
        conditional(SUMIFS, "SUMIFS", (3, 255), SUM_PAIRS, sumifs),
        conditional(AVERAGEIF, "AVERAGEIF", (2, 3), AVERAGE_RANGE, averageif),
        conditional(
            AVERAGEIFS,
            "AVERAGEIFS",
            (3, 255),
            AVERAGE_PAIRS,
            averageifs,
        ),
        conditional(MAXIFS, "MAXIFS", (3, 255), MAX_PAIRS, maxifs),
        conditional(MINIFS, "MINIFS", (3, 255), MIN_PAIRS, minifs),
    ],
};

/// COUNTIF: the cells of the range that meet the criterion.
fn countif(
    arguments: &[CalcValue],
    services: &mut EvalServices<'_>,
) -> Result<CalcValue, FunctionFault> {
    let site = Site::new(COUNTIF, services);
    Site::finish(single(&site, arguments, services, false).and_then(|plan| {
        let conditions = plan.read_conditions(&site, services)?;
        count(&site, &conditions, plan.size)
    }))
}

/// COUNTIFS: the positions at which every criteria range's cell meets its criterion.
fn countifs(
    arguments: &[CalcValue],
    services: &mut EvalServices<'_>,
) -> Result<CalcValue, FunctionFault> {
    let site = Site::new(COUNTIFS, services);
    Site::finish(pairs(&site, arguments, services, false).and_then(|plan| {
        let conditions = plan.read_conditions(&site, services)?;
        count(&site, &conditions, plan.size)
    }))
}

/// SUMIF: the numbers of the sum range, or of the range when it is omitted, in the rows whose
/// range cell meets the criterion, added from 0. A total that overflows is #NUM!.
fn sumif(
    arguments: &[CalcValue],
    services: &mut EvalServices<'_>,
) -> Result<CalcValue, FunctionFault> {
    let site = Site::new(SUMIF, services);
    Site::finish(
        single(&site, arguments, services, true).and_then(|plan| total(&site, &plan, services)),
    )
}

/// SUMIFS: SUMIF over the rows that meet every criterion.
fn sumifs(
    arguments: &[CalcValue],
    services: &mut EvalServices<'_>,
) -> Result<CalcValue, FunctionFault> {
    let site = Site::new(SUMIFS, services);
    Site::finish(
        pairs(&site, arguments, services, true).and_then(|plan| total(&site, &plan, services)),
    )
}

/// AVERAGEIF: the matched numbers added from 0 and divided by how many there are; #DIV/0! when
/// there is none.
fn averageif(
    arguments: &[CalcValue],
    services: &mut EvalServices<'_>,
) -> Result<CalcValue, FunctionFault> {
    let site = Site::new(AVERAGEIF, services);
    Site::finish(
        single(&site, arguments, services, true).and_then(|plan| mean(&site, &plan, services)),
    )
}

/// AVERAGEIFS: AVERAGEIF over the rows that meet every criterion.
fn averageifs(
    arguments: &[CalcValue],
    services: &mut EvalServices<'_>,
) -> Result<CalcValue, FunctionFault> {
    let site = Site::new(AVERAGEIFS, services);
    Site::finish(
        pairs(&site, arguments, services, true).and_then(|plan| mean(&site, &plan, services)),
    )
}

/// MAXIFS: the largest matched number, or 0 when there is none.
fn maxifs(
    arguments: &[CalcValue],
    services: &mut EvalServices<'_>,
) -> Result<CalcValue, FunctionFault> {
    let site = Site::new(MAXIFS, services);
    Site::finish(
        pairs(&site, arguments, services, true)
            .and_then(|plan| extreme(&site, &plan, services, |n, best| n > best)),
    )
}

/// MINIFS: the smallest matched number, or 0 when there is none.
fn minifs(
    arguments: &[CalcValue],
    services: &mut EvalServices<'_>,
) -> Result<CalcValue, FunctionFault> {
    let site = Site::new(MINIFS, services);
    Site::finish(
        pairs(&site, arguments, services, true)
            .and_then(|plan| extreme(&site, &plan, services, |n, best| n < best)),
    )
}

fn total(site: &Site<'_>, plan: &Plan, services: &mut EvalServices<'_>) -> Step<CalcValue> {
    let mut total = 0.0;
    plan.each_matched(site, services, &mut |n| {
        total = site.add(total, n)?;
        Ok(())
    })?;
    site.number(total)
}

fn mean(site: &Site<'_>, plan: &Plan, services: &mut EvalServices<'_>) -> Step<CalcValue> {
    let (mut total, mut counted) = (0.0, 0u64);
    plan.each_matched(site, services, &mut |n| {
        total = site.add(total, n)?;
        counted += 1;
        Ok(())
    })?;
    // A count is below 2^53, so it converts to binary64 exactly.
    site.number(site.divide(total, counted as f64)?)
}

fn extreme(
    site: &Site<'_>,
    plan: &Plan,
    services: &mut EvalServices<'_>,
    replaces: fn(f64, f64) -> bool,
) -> Step<CalcValue> {
    let mut best: Option<f64> = None;
    plan.each_matched(site, services, &mut |n| {
        if best.is_none_or(|best| replaces(n, best)) {
            best = Some(n);
        }
        Ok(())
    })?;
    site.number(best.unwrap_or(0.0))
}

// ---------------------------------------------------------------------------------------------
// What a call reads.

/// A range argument: its argument index and its one area.
#[derive(Clone, Copy)]
struct Range {
    index: usize,
    area: GridArea,
}

impl Range {
    fn shape(self) -> (u32, u32) {
        (self.area.row_count(), self.area.column_count())
    }
}

/// A call's criteria ranges with their criteria, its target range (for every function but the
/// counts) and the number of cells of each range.
struct Plan {
    conditions: Vec<(Range, Criterion)>,
    target: Option<Range>,
    size: u64,
}

/// The reference argument at `index` as a range of one area.
fn range(site: &Site<'_>, arguments: &[CalcValue], index: usize) -> Step<Range> {
    let CoreValue::Reference(reference) = arguments[index].core() else {
        return Err(Exit::Fault(FunctionFault::ReferenceRequired {
            id: site.id(),
            index,
        }));
    };
    match reference.areas() {
        [area] => Ok(Range { index, area: *area }),
        _ => Err(Exit::Error(WorksheetErrorCode::Value)),
    }
}

/// The plan of COUNTIF (no target), SUMIF and AVERAGEIF: the range, its criterion, and the
/// target, which is the range when the third argument is omitted and otherwise the third
/// argument's top-left cell extended to the range's size, cut at the sheet's last row; one cut at
/// the last column is not modelled (see the module documentation).
fn single(
    site: &Site<'_>,
    arguments: &[CalcValue],
    services: &mut EvalServices<'_>,
    with_target: bool,
) -> Step<Plan> {
    let criteria_range = range(site, arguments, 0)?;
    let target = match (with_target, arguments.get(2)) {
        (false, _) => None,
        (true, None) => Some(criteria_range),
        (true, Some(_)) => {
            let given = range(site, arguments, 2)?;
            let (rows, columns) = criteria_range.shape();
            let first = given.area.first();
            let last_column = u64::from(first.column()) + u64::from(columns) - 1;
            if last_column > u64::from(MAX_COLUMNS) {
                return Err(site.not_modelled(Some(2)));
            }
            let last_row = (u64::from(first.row()) + u64::from(rows) - 1).min(u64::from(MAX_ROWS));
            let last = CellKey::new(first.sheet(), last_row as u32, last_column as u32)
                .expect("a cell on the sheet");
            let area = GridArea::new(first, last).expect("an area from the given first cell");
            Some(Range { index: 2, area })
        }
    };
    let criterion = criterion(site, services, 1, &arguments[1])?;
    Ok(Plan {
        conditions: vec![(criteria_range, criterion)],
        target,
        size: criteria_range.area.cell_count(),
    })
}

/// The plan of an *IFS function: its pairs from argument 1 behind the target range (argument 0)
/// when `with_target`, from argument 0 otherwise. A count of arguments that does not form pairs
/// is an arity the entry does not admit. Every range must have the first range's shape.
fn pairs(
    site: &Site<'_>,
    arguments: &[CalcValue],
    services: &mut EvalServices<'_>,
    with_target: bool,
) -> Step<Plan> {
    let first_pair = usize::from(with_target);
    if !(arguments.len() - first_pair).is_multiple_of(2) {
        return Err(Exit::Fault(FunctionFault::ArityMismatch {
            id: site.id(),
            supplied: arguments.len(),
        }));
    }
    let target = if with_target {
        Some(range(site, arguments, 0)?)
    } else {
        None
    };
    let mut ranges = Vec::with_capacity((arguments.len() - first_pair) / 2);
    for index in (first_pair..arguments.len()).step_by(2) {
        ranges.push(range(site, arguments, index)?);
    }
    let shape = target.unwrap_or(ranges[0]).shape();
    if ranges.iter().any(|range| range.shape() != shape) {
        return Err(Exit::Error(WorksheetErrorCode::Value));
    }
    let mut conditions = Vec::with_capacity(ranges.len());
    for range in ranges {
        let index = range.index + 1;
        conditions.push((range, criterion(site, services, index, &arguments[index])?));
    }
    Ok(Plan {
        conditions,
        target,
        size: u64::from(shape.0) * u64::from(shape.1),
    })
}

/// One criteria range's verdicts: at each cell the reader visited with a value, by row-major
/// offset in increasing order, and at a blank cell.
struct Verdicts {
    index: usize,
    held: Vec<(u64, Match)>,
    blank: Match,
}

/// A target cell that a matched row uses: a number, or an error, which is the result.
#[derive(Clone, Copy)]
enum Used {
    Number(f64),
    Error(WorksheetErrorCode),
}

impl Plan {
    /// Reads every criteria range, in argument order, into its verdicts.
    fn read_conditions(
        &self,
        site: &Site<'_>,
        services: &mut EvalServices<'_>,
    ) -> Step<Vec<Verdicts>> {
        let mut all = Vec::with_capacity(self.conditions.len());
        for (range, criterion) in &self.conditions {
            all.push(verdicts(site, services, *range, criterion, None)?);
        }
        Ok(all)
    }

    /// Calls `take` with every number of the target in a row that every criterion matches, in
    /// row-major order; the first error in such a row is the result.
    fn each_matched(
        &self,
        site: &Site<'_>,
        services: &mut EvalServices<'_>,
        take: &mut dyn FnMut(f64) -> Step<()>,
    ) -> Step<()> {
        let target = self.target.expect("a function with a target range");
        let mut used: Option<Vec<(u64, Used)>> = None;
        let mut conditions = Vec::with_capacity(self.conditions.len());
        for (range, criterion) in &self.conditions {
            let keep = used.is_none() && range.area == target.area;
            let mut kept = Vec::new();
            conditions.push(verdicts(
                site,
                services,
                *range,
                criterion,
                keep.then_some(&mut kept),
            )?);
            if keep {
                used = Some(kept);
            }
        }
        let used = match used {
            Some(used) => used,
            None => target_cells(site, services, target)?,
        };
        let mut cursors = vec![0usize; conditions.len()];
        for (offset, cell) in used {
            match row(&conditions, &mut cursors, offset) {
                Row::Yes => match cell {
                    Used::Number(n) => take(n)?,
                    Used::Error(code) => return Err(Exit::Error(code)),
                },
                Row::No => {}
                Row::Unknown(index) => return Err(site.not_modelled(Some(index))),
            }
        }
        Ok(())
    }
}

/// The verdict of one row across the criteria ranges.
enum Row {
    Yes,
    No,
    /// No criterion rules the row out, and the verdict of the range at this argument is unknown.
    Unknown(usize),
}

/// The verdict of the row at `offset`, at or past every cursor's position: each range's verdict
/// at a cell it holds there, else its verdict on a blank. Moves each cursor to its first held
/// cell at or past `offset`.
fn row(conditions: &[Verdicts], cursors: &mut [usize], offset: u64) -> Row {
    let mut unknown = None;
    for (verdicts, cursor) in conditions.iter().zip(cursors.iter_mut()) {
        while verdicts
            .held
            .get(*cursor)
            .is_some_and(|&(at, _)| at < offset)
        {
            *cursor += 1;
        }
        let verdict = match verdicts.held.get(*cursor) {
            Some(&(at, verdict)) if at == offset => verdict,
            _ => verdicts.blank,
        };
        match verdict {
            Match::No => return Row::No,
            Match::Unknown => {
                unknown.get_or_insert(verdicts.index);
            }
            Match::Yes => {}
        }
    }
    match unknown {
        Some(index) => Row::Unknown(index),
        None => Row::Yes,
    }
}

/// The positions, among `size`, at which every range's verdict is yes. The rows where some range
/// holds a value are taken one by one, in offset order; the rest are blank in every range and are
/// counted at once.
fn count(site: &Site<'_>, conditions: &[Verdicts], size: u64) -> Step<CalcValue> {
    let mut cursors = vec![0usize; conditions.len()];
    let (mut counted, mut held) = (0u64, 0u64);
    while let Some(offset) = conditions
        .iter()
        .zip(&cursors)
        .filter_map(|(verdicts, &cursor)| verdicts.held.get(cursor).map(|&(at, _)| at))
        .min()
    {
        held += 1;
        match row(conditions, &mut cursors, offset) {
            Row::Yes => counted += 1,
            Row::No => {}
            Row::Unknown(index) => return Err(site.not_modelled(Some(index))),
        }
        for (verdicts, cursor) in conditions.iter().zip(cursors.iter_mut()) {
            if verdicts
                .held
                .get(*cursor)
                .is_some_and(|&(at, _)| at == offset)
            {
                *cursor += 1;
            }
        }
    }
    if conditions
        .iter()
        .all(|verdicts| verdicts.blank == Match::Yes)
    {
        counted += size - held;
    }
    // At most 2^34 cells, which binary64 holds exactly.
    site.number(counted as f64)
}

/// The row-major offset of `key` in `area`.
fn offset(area: GridArea, key: CellKey) -> u64 {
    let first = area.first();
    u64::from(key.row() - first.row()) * u64::from(area.column_count())
        + u64::from(key.column() - first.column())
}

/// Reads `range` through the cell reader, calling `visit` with the offset and value of each cell
/// visited with a value, in increasing offset order. A reader that visits a cell outside the area
/// or out of row-major order cannot supply it.
fn read_range(
    site: &Site<'_>,
    services: &mut EvalServices<'_>,
    range: Range,
    visit: &mut dyn FnMut(u64, &CalcValue) -> Step<()>,
) -> Step<()> {
    let cells = reader(site, services, range.index)?;
    let reference = ReferenceLike::area(range.area);
    let mut last: Option<u64> = None;
    read_reference(
        site,
        cells,
        &reference,
        range.index,
        &mut |area, key, value| {
            if matches!(value.core(), CoreValue::Empty | CoreValue::Missing) {
                return Ok(());
            }
            let at = offset(area, key);
            if last.is_some_and(|last| last >= at) {
                return Err(Exit::Fault(FunctionFault::ReferenceUnreadable {
                    id: site.id(),
                    index: range.index,
                }));
            }
            last = Some(at);
            visit(at, value)
        },
    )
}

/// Reads a criteria range into its verdicts under `criterion`; with `kept`, also keeps the cells
/// a target uses, when the target is this range.
fn verdicts(
    site: &Site<'_>,
    services: &mut EvalServices<'_>,
    range: Range,
    criterion: &Criterion,
    mut kept: Option<&mut Vec<(u64, Used)>>,
) -> Step<Verdicts> {
    let mut held = Vec::new();
    read_range(site, services, range, &mut |at, value| {
        held.push((at, criterion.verdict(site, range.index, value)?));
        if let Some(kept) = kept.as_deref_mut()
            && let Some(cell) = used(site, range.index, value)?
        {
            kept.push((at, cell));
        }
        Ok(())
    })?;
    Ok(Verdicts {
        index: range.index,
        held,
        blank: criterion.verdict(site, range.index, &CalcValue::empty())?,
    })
}

/// Reads the target range: the cells a matched row would use, by offset.
fn target_cells(
    site: &Site<'_>,
    services: &mut EvalServices<'_>,
    target: Range,
) -> Step<Vec<(u64, Used)>> {
    let mut kept = Vec::new();
    read_range(site, services, target, &mut |at, value| {
        if let Some(cell) = used(site, target.index, value)? {
            kept.push((at, cell));
        }
        Ok(())
    })?;
    Ok(kept)
}

/// What a target cell gives a matched row: a number, an error, or nothing (text, logicals and
/// blank cells are skipped).
fn used(site: &Site<'_>, index: usize, value: &CalcValue) -> Step<Option<Used>> {
    match value.core() {
        CoreValue::Number(n) => Ok(Some(Used::Number(
            site.argument(index, coerce::operand(*n))?,
        ))),
        CoreValue::Error(code) => Ok(Some(Used::Error(*code))),
        CoreValue::Text(_) | CoreValue::Logical(_) | CoreValue::Empty | CoreValue::Missing => {
            Ok(None)
        }
        CoreValue::Array(_) | CoreValue::Reference(_) => Err(site.not_modelled(Some(index))),
    }
}

// ---------------------------------------------------------------------------------------------
// The criteria language.

/// A cell's verdict under a criterion.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Match {
    Yes,
    No,
    /// Excel's verdict needs a rule that is not modelled.
    Unknown,
}

impl Match {
    fn of(holds: bool) -> Self {
        if holds { Match::Yes } else { Match::No }
    }

    fn negated(self) -> Self {
        match self {
            Match::Yes => Match::No,
            Match::No => Match::Yes,
            Match::Unknown => Match::Unknown,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Op {
    Equal,
    NotEqual,
    Less,
    LessOrEqual,
    Greater,
    GreaterOrEqual,
}

impl Op {
    fn holds(self, ordering: Ordering) -> bool {
        match self {
            Op::Equal => ordering.is_eq(),
            Op::NotEqual => ordering.is_ne(),
            Op::Less => ordering.is_lt(),
            Op::LessOrEqual => ordering.is_le(),
            Op::Greater => ordering.is_gt(),
            Op::GreaterOrEqual => ordering.is_ge(),
        }
    }

    fn is_equality(self) -> bool {
        matches!(self, Op::Equal | Op::NotEqual)
    }
}

/// One element of a pattern.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Token {
    /// This code unit, ASCII case ignored.
    Unit(u16),
    /// `?`: any one character.
    One,
    /// `*`: any run of characters, the empty run included.
    Run,
}

/// A criterion as read from its argument.
#[derive(Debug)]
pub(super) enum Criterion {
    /// `""`: blank cells and empty text.
    BlankOrEmptyText,
    /// `=`: blank cells.
    Blank,
    /// `<>`: every cell but a blank one.
    NotBlank,
    /// `<`, `<=`, `>` or `>=` with no operand: no cell.
    Never,
    Number(Op, f64),
    Logical(Op, bool),
    Error(Op, WorksheetErrorCode),
    /// Text, its escapes resolved: equality, or an order.
    Text(Op, Vec<u16>),
    /// Equality only.
    Pattern(Op, Vec<Token>),
}

/// The error literals a criterion may name, as the formula language spells them, in the order an
/// ordering criterion compares errors. A criterion naming an error that has no literal here is
/// not modelled.
const ERROR_CRITERIA: [(&str, WorksheetErrorCode); 7] = [
    ("#NULL!", WorksheetErrorCode::Null),
    ("#DIV/0!", WorksheetErrorCode::Div0),
    ("#VALUE!", WorksheetErrorCode::Value),
    ("#REF!", WorksheetErrorCode::Ref),
    ("#NAME?", WorksheetErrorCode::Name),
    ("#NUM!", WorksheetErrorCode::Num),
    ("#N/A", WorksheetErrorCode::NA),
];
const OTHER_ERRORS: [&str; 7] = [
    "#GETTING_DATA",
    "#BLOCKED!",
    "#CONNECT!",
    "#FIELD!",
    "#SPILL!",
    "#CALC!",
    "#BUSY!",
];

const TRUE_TEXT: &str = "TRUE";
const FALSE_TEXT: &str = "FALSE";

fn spells(units: &[u16], word: &str) -> bool {
    units.len() == word.len()
        && units.iter().zip(word.bytes()).all(|(&unit, byte)| {
            u8::try_from(unit).is_ok_and(|unit| unit.to_ascii_uppercase() == byte)
        })
}

/// The criterion argument at `index`, dereferenced when it is a reference to one cell.
fn criterion(
    site: &Site<'_>,
    services: &mut EvalServices<'_>,
    index: usize,
    argument: &CalcValue,
) -> Step<Criterion> {
    let dereferenced;
    let value = match argument.core() {
        CoreValue::Reference(reference) => {
            let [area] = reference.areas() else {
                return Err(site.not_modelled(Some(index)));
            };
            if area.cell_count() != 1 {
                // A criterion of several cells lifts the function over them: not modelled.
                return Err(site.not_modelled(Some(index)));
            }
            let mut found = CalcValue::empty();
            read_range(
                site,
                services,
                Range { index, area: *area },
                &mut |_, value| {
                    found = value.clone();
                    Ok(())
                },
            )?;
            dereferenced = found;
            &dereferenced
        }
        _ => argument,
    };
    match value.core() {
        CoreValue::Number(n) => Ok(Criterion::Number(
            Op::Equal,
            site.argument(index, coerce::operand(*n))?,
        )),
        CoreValue::Logical(b) => Ok(Criterion::Logical(Op::Equal, *b)),
        CoreValue::Error(code) => Ok(Criterion::Error(Op::Equal, *code)),
        // An empty cell given as the criterion, and an omitted criterion, are the value 0.
        CoreValue::Empty | CoreValue::Missing => Ok(Criterion::Number(Op::Equal, 0.0)),
        CoreValue::Text(text) => criterion_text(site, index, text.utf16_code_units()),
        CoreValue::Array(_) | CoreValue::Reference(_) => Err(site.not_modelled(Some(index))),
    }
}

/// A criteria cell of a database function (DSUM and the others): a blank cell sets no condition
/// (`None`); text with no operator that is neither a number nor a logical matches the cells
/// whose text begins with it, as Microsoft's database function pages state; anything else is read
/// as a criterion of the *IFS functions.
pub(super) fn database_criterion(
    site: &Site<'_>,
    index: usize,
    value: &CalcValue,
) -> Step<Option<Criterion>> {
    match value.core() {
        CoreValue::Empty | CoreValue::Missing => Ok(None),
        CoreValue::Text(text) if text.len_utf16_code_units() == 0 => Ok(None),
        CoreValue::Text(text) => {
            let units = text.utf16_code_units();
            let written = [b'<', b'>', b'='].iter().any(|&b| units[0] == u16::from(b));
            let criterion = criterion_text(site, index, units)?;
            Ok(Some(match criterion {
                Criterion::Text(Op::Equal, literal) if !written => {
                    let mut tokens: Vec<Token> = literal.into_iter().map(Token::Unit).collect();
                    tokens.push(Token::Run);
                    Criterion::Pattern(Op::Equal, tokens)
                }
                Criterion::Pattern(Op::Equal, mut tokens) if !written => {
                    if tokens.last() != Some(&Token::Run) {
                        tokens.push(Token::Run);
                    }
                    Criterion::Pattern(Op::Equal, tokens)
                }
                other => other,
            }))
        }
        CoreValue::Number(n) => Ok(Some(Criterion::Number(
            Op::Equal,
            site.argument(index, coerce::operand(*n))?,
        ))),
        CoreValue::Logical(b) => Ok(Some(Criterion::Logical(Op::Equal, *b))),
        CoreValue::Error(code) => Err(Exit::Error(*code)),
        CoreValue::Array(_) | CoreValue::Reference(_) => Err(site.not_modelled(Some(index))),
    }
}

/// An error's place in the order of [`ERROR_CRITERIA`], when it has a literal there.
fn rank(code: WorksheetErrorCode) -> Option<usize> {
    ERROR_CRITERIA.iter().position(|(_, known)| *known == code)
}

/// A criterion written as text; see the module documentation.
fn criterion_text(site: &Site<'_>, index: usize, units: &[u16]) -> Step<Criterion> {
    if units.len() > LONGEST_TEXT {
        return Err(Exit::Error(WorksheetErrorCode::Value));
    }
    let starts = |prefix: &str| {
        units.len() >= prefix.len()
            && units
                .iter()
                .zip(prefix.bytes())
                .all(|(&u, b)| u == u16::from(b))
    };
    let (op, written, rest) = if starts("<=") {
        (Op::LessOrEqual, true, &units[2..])
    } else if starts(">=") {
        (Op::GreaterOrEqual, true, &units[2..])
    } else if starts("<>") {
        (Op::NotEqual, true, &units[2..])
    } else if starts("<") {
        (Op::Less, true, &units[1..])
    } else if starts(">") {
        (Op::Greater, true, &units[1..])
    } else if starts("=") {
        (Op::Equal, true, &units[1..])
    } else {
        (Op::Equal, false, units)
    };
    if rest.is_empty() {
        return Ok(match (op, written) {
            (Op::Equal, false) => Criterion::BlankOrEmptyText,
            (Op::Equal, true) => Criterion::Blank,
            (Op::NotEqual, _) => Criterion::NotBlank,
            _ => Criterion::Never,
        });
    }
    if let Some(&(_, code)) = ERROR_CRITERIA
        .iter()
        .find(|(spelling, _)| spells(rest, spelling))
    {
        return Ok(Criterion::Error(op, code));
    }
    if OTHER_ERRORS.iter().any(|spelling| spells(rest, spelling)) {
        return Err(site.not_modelled(Some(index)));
    }
    let special = |unit: &u16| [b'*', b'?', b'~'].iter().any(|&b| *unit == u16::from(b));
    if op.is_equality() && rest.iter().any(special) {
        let mut tokens = Vec::with_capacity(rest.len());
        let mut at = 0;
        while at < rest.len() {
            let unit = rest[at];
            tokens.push(if unit == u16::from(b'~') {
                match rest.get(at + 1) {
                    Some(&next) if special(&next) => {
                        at += 1;
                        Token::Unit(next)
                    }
                    // Before any other character, or at the end, ~ is itself.
                    _ => Token::Unit(unit),
                }
            } else if unit == u16::from(b'*') {
                Token::Run
            } else if unit == u16::from(b'?') {
                Token::One
            } else {
                Token::Unit(unit)
            });
            at += 1;
        }
        if tokens.iter().all(|token| matches!(token, Token::Unit(_))) {
            let literal = tokens
                .into_iter()
                .map(|token| match token {
                    Token::Unit(unit) => unit,
                    Token::One | Token::Run => unreachable!("only literal units remain"),
                })
                .collect();
            return Ok(Criterion::Text(op, literal));
        }
        return Ok(Criterion::Pattern(op, tokens));
    }
    let operand = CalcValue::text(ExcelText::from_utf16_code_units(rest.to_vec()));
    match site.number_argument(index, &operand) {
        Ok(n) => Ok(Criterion::Number(op, n)),
        // Text that reads as no number: no digit, out of the range Excel reads, or a word outside
        // the words coercion's measured list lets number text hold (such as the label 2016-17
        // Winter).
        Err(Exit::Error(_)) => {
            let logical = if spells(rest, TRUE_TEXT) {
                Some(true)
            } else if spells(rest, FALSE_TEXT) {
                Some(false)
            } else {
                None
            };
            Ok(match logical {
                Some(b) => Criterion::Logical(op, b),
                None => Criterion::Text(op, rest.to_vec()),
            })
        }
        Err(exit) => Err(exit),
    }
}

/// A comparison's step as a verdict: a refusal as not modelled is an unknown verdict, and any
/// other fault (ExactUnavailable) stops the call.
fn decided(step: Step<bool>) -> Step<Match> {
    match step {
        Ok(holds) => Ok(Match::of(holds)),
        Err(Exit::Fault(FunctionFault::NotImplemented { .. })) => Ok(Match::Unknown),
        Err(exit) => Err(exit),
    }
}

impl Criterion {
    /// The verdict of a cell of the range at argument `index` holding `value` (Empty for a blank
    /// cell).
    pub(super) fn verdict(&self, site: &Site<'_>, index: usize, value: &CalcValue) -> Step<Match> {
        if let CoreValue::Number(n) = value.core() {
            site.argument(index, coerce::operand(*n))?;
        }
        let blank = matches!(value.core(), CoreValue::Empty | CoreValue::Missing);
        match self {
            Criterion::BlankOrEmptyText => Ok(Match::of(
                blank
                    || value
                        .as_text()
                        .is_some_and(|t| t.len_utf16_code_units() == 0),
            )),
            Criterion::Blank => Ok(Match::of(blank)),
            Criterion::NotBlank => Ok(Match::of(!blank)),
            Criterion::Never => Ok(Match::No),
            Criterion::Number(op, operand) => {
                let cell = match value.core() {
                    CoreValue::Number(n) => Some(*n),
                    // Text is never a number to an ordering or to <>; only = reads it as one.
                    CoreValue::Text(_) if *op != Op::Equal => None,
                    CoreValue::Text(_) => match site.number_argument(index, value) {
                        Ok(n) => Some(n),
                        // Text that reads as no number (see the criterion's operand above).
                        Err(Exit::Error(_)) => None,
                        Err(exit) => return decided(Err(exit)),
                    },
                    _ => None,
                };
                let verdict = match cell {
                    Some(n) => decided(
                        comparison::numbers(site, n, *operand).map(|ordering| op.holds(ordering)),
                    )?,
                    None => Match::of(*op == Op::NotEqual),
                };
                Ok(verdict)
            }
            Criterion::Logical(op, operand) => Ok(match value.core() {
                CoreValue::Logical(b) => Match::of(op.holds(b.cmp(operand))),
                CoreValue::Text(text)
                    if spells(text.utf16_code_units(), TRUE_TEXT)
                        || spells(text.utf16_code_units(), FALSE_TEXT) =>
                {
                    Match::Unknown
                }
                _ => Match::of(*op == Op::NotEqual),
            }),
            Criterion::Error(op, operand) => Ok(match value.core() {
                CoreValue::Error(code) => {
                    let ordering = if code == operand {
                        Some(Ordering::Equal)
                    } else if op.is_equality() {
                        Some(Ordering::Less)
                    } else {
                        rank(*code)
                            .zip(rank(*operand))
                            .map(|(code, operand)| code.cmp(&operand))
                    };
                    ordering.map_or(Match::Unknown, |ordering| Match::of(op.holds(ordering)))
                }
                CoreValue::Text(text)
                    if ERROR_CRITERIA.iter().any(|(spelling, code)| {
                        (code == operand || !op.is_equality())
                            && spells(text.utf16_code_units(), spelling)
                    }) =>
                {
                    Match::Unknown
                }
                _ => Match::of(*op == Op::NotEqual),
            }),
            Criterion::Text(op, operand) => {
                let Some(text) = value.as_text() else {
                    return Ok(Match::of(*op == Op::NotEqual));
                };
                let units = text.utf16_code_units();
                if units.len() > LONGEST_TEXT {
                    return Ok(Match::Unknown);
                }
                let want = if op.is_equality() {
                    Want::Equality
                } else {
                    Want::Order
                };
                Ok(match comparison::texts(units, operand, want) {
                    Ok(ordering) => Match::of(op.holds(ordering)),
                    Err(_) => Match::Unknown,
                })
            }
            Criterion::Pattern(op, tokens) => {
                let equal = match value.as_text() {
                    None => Match::No,
                    Some(text) => {
                        let units = text.utf16_code_units();
                        if units.len() > LONGEST_TEXT {
                            Match::Unknown
                        } else if tokens.iter().all(|token| *token == Token::Run) {
                            Match::Yes
                        } else if units.iter().any(|&unit| is_surrogate(unit))
                            || tokens.iter().any(
                                |token| matches!(token, Token::Unit(unit) if is_surrogate(*unit)),
                            )
                        {
                            // A pattern over text holding a character beyond the BMP: whether ?
                            // takes one character or one code unit there is not modelled.
                            Match::Unknown
                        } else {
                            Match::of(wildcard(tokens, units))
                        }
                    }
                };
                Ok(equality(*op, equal))
            }
        }
    }
}

/// An equality verdict under `op`, which is = or <>.
fn equality(op: Op, equal: Match) -> Match {
    if op == Op::NotEqual {
        equal.negated()
    } else {
        equal
    }
}

fn is_surrogate(unit: u16) -> bool {
    (0xD800..=0xDFFF).contains(&unit)
}

/// Whether `text` (no surrogates) matches `pattern`, case ignored character by character through
/// the simple lower-case mapping: Excel 16.0.20430 matched é* to Élan, ασ* to ΑΣ and ǆ* to ǅemal,
/// and matched neither *SSE to Straße nor nai* to naïve (read ad hoc on this machine, dc-ip6).
fn wildcard(pattern: &[Token], text: &[u16]) -> bool {
    let fold = crate::ported::text_kernels::fold_unit;
    let (mut p, mut t) = (0, 0);
    // The last * met: its position in the pattern and the text position its run ends at.
    let mut star: Option<(usize, usize)> = None;
    while t < text.len() {
        match pattern.get(p) {
            Some(Token::Run) => {
                star = Some((p, t));
                p += 1;
                continue;
            }
            Some(Token::One) => {
                p += 1;
                t += 1;
                continue;
            }
            Some(Token::Unit(unit)) if fold(*unit) == fold(text[t]) => {
                p += 1;
                t += 1;
                continue;
            }
            _ => {}
        }
        match star {
            Some((at, end)) => {
                p = at + 1;
                t = end + 1;
                star = Some((at, end + 1));
            }
            None => return false,
        }
    }
    pattern[p..].iter().all(|token| *token == Token::Run)
}
