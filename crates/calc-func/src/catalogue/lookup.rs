//! The lookup and reference family fragment: INDEX, MATCH, XMATCH, XLOOKUP, VLOOKUP, HLOOKUP,
//! LOOKUP, CHOOSE, ROW, COLUMN, ROWS, COLUMNS and AREAS, and their kernels.
//!
//! A lookup searches one row or one column of cells: part of a reference's single area, read
//! through the services' cell reader, which visits the cells that hold a value (a cell it does not
//! visit is blank), or part of an array argument. Every read stays inside the reference the call
//! is given, and the work and memory of a search follow the cells the reader visits, not the
//! area's size, so a whole column costs what it holds.
//!
//! Matching sees a value as one of five kinds: a number, text, a logical, an error or blank. Two
//! values match only when they are of one kind and equal: numbers by their exact binary64 value
//! (no near-equality), logicals by value, and text as calc_func's comparison operators equate it,
//! ignoring case, refused where that comparison cannot decide. Text that reads as a number is
//! text: "2" never matches 2. A text lookup value longer than 255 UTF-16 code units is #VALUE! for
//! MATCH, VLOOKUP, HLOOKUP and LOOKUP.
//!
//! MATCH with match_type 0, VLOOKUP and HLOOKUP in exact mode and XMATCH and XLOOKUP with
//! match_mode 2 read a text lookup value that holds `*`, `?` or `~` as a pattern: `*` matches any
//! run of characters, `?` one character, and `~` takes the next character literally (a `~` at the
//! end is dropped); a pattern matches text cells only. These searches match a text lookup value
//! that holds none only to an equal text cell of the same length in UTF-16 code units: there ss
//! does not match U+00DF, nor does a text match itself with a soft hyphen added, while XMATCH and
//! XLOOKUP in exact mode, and the approximate searches, take such texts as equal. The MATCH family
//! (MATCH, VLOOKUP, HLOOKUP, LOOKUP) reads a blank or omitted lookup value as the number 0; XMATCH
//! and XLOOKUP read it as blank, which matches blank cells.
//!
//! The approximate searches follow the results Excel gives on unsorted data, not only on sorted
//! data:
//! - MATCH with match_type 1, VLOOKUP and HLOOKUP in approximate mode and LOOKUP bisect the line,
//!   seeing only the cells of the lookup value's kind: at each midpoint the first such cell at or
//!   after it decides (when there is none before the end of the range, the range ends before the
//!   midpoint); an equal cell ends the search at the last of the equal cells that follow it
//!   without a gap, a smaller one moves the range after it, a larger one before it, and the
//!   result is the last smaller cell met.
//! - MATCH with match_type -1 walks the line from the start over the cells of the lookup value's
//!   kind: an equal cell is the result, a larger cell is remembered, and the walk stops at the
//!   first smaller cell; the result is the last larger cell remembered.
//! - XMATCH and XLOOKUP order every value: numbers, then text, then logicals, then errors (#NULL!,
//!   #DIV/0!, #VALUE!, #REF!, #NAME?, #NUM!, #N/A, #GETTING_DATA, #SPILL!, #FIELD!, #CALC!), then
//!   blank cells. Their linear modes take an equal cell when there is one, else the nearest
//!   smaller or larger value, the first of equal candidates in the search's direction: the kind
//!   nearest the lookup value decides first, and candidates are ordered within a kind only when
//!   that kind holds the result; their binary modes bisect over every cell in that order, an equal
//!   cell moving the range toward the start (ascending) or the end (descending) to find the first
//!   or last equal cell. -0 is ordered as +0, as matching compares it.
//!
//! An order a result needs between texts that calc_func's comparison cannot decide, or between
//! #BUSY!, #BLOCKED! or #CONNECT! and another error, regex matching (match_mode 3), an array
//! lookup value or index (Excel lifts the function over it), ROW or COLUMN of several rows or
//! columns (whose result depends on the formula's array context) and a read past the reference
//! given are refused as not modelled.
//!
//! INDEX returns a reference when its first argument is one, and XLOOKUP and CHOOSE return what
//! they select as it is, so a reference result stays a reference until the consuming context
//! dereferences it. VLOOKUP, HLOOKUP and LOOKUP return the cell's value: a blank cell is Empty.

use super::comparison::{self, Want};
use crate::call::{CellReader, EvalServices, FunctionFault, LazyArguments, Unreadable};
use crate::coerce;
use crate::kernel::{Exit, Site, Step};
use crate::value::{
    ArrayShape, CalcArray, CalcValue, CellKey, CoreRef, GridArea, ReferenceLike, WorksheetErrorCode,
};
use std::cmp::Ordering;
use std::ops::ControlFlow;

use calc_catalogue::lookup::{
    AREAS, CHOOSE, CODE, COLUMN, COLUMNS, ENTRIES as DECLARATIONS, HLOOKUP, INDEX, LOOKUP, MATCH,
    ROW, ROWS, VLOOKUP, XLOOKUP, XMATCH,
};

attach_family! {
    code: CODE; declarations: DECLARATIONS;
    INDEX => Eager(index);
    MATCH => Eager(match_);
    XMATCH => Eager(xmatch);
    XLOOKUP => Eager(xlookup);
    VLOOKUP => Eager(vlookup);
    HLOOKUP => Eager(hlookup);
    LOOKUP => Eager(lookup);
    CHOOSE => Lazy(choose);
    ROW => Eager(row);
    COLUMN => Eager(column);
    ROWS => Eager(rows);
    COLUMNS => Eager(columns);
    AREAS => Eager(areas);
}

// ---------------------------------------------------------------------------------------------
// Values as matching sees them.

/// A lookup value or a held value, as matching sees it.
#[derive(Clone, Copy, Debug)]
enum Item<'a> {
    Number(f64),
    Text(&'a [u16]),
    Logical(bool),
    Error(WorksheetErrorCode),
    Blank,
}

impl Item<'_> {
    /// The kind's place in XMATCH's order: numbers, text, logicals, errors, blank cells.
    fn rank(self) -> u8 {
        match self {
            Item::Number(_) => 0,
            Item::Text(_) => 1,
            Item::Logical(_) => 2,
            Item::Error(_) => 3,
            Item::Blank => 4,
        }
    }
}

/// A held value as an item. A blank cell is Blank; an array or a reference held in a cell or an
/// array, and a number that is not a worksheet number, are refused naming argument `index`.
fn item<'v>(site: &Site<'_>, index: usize, value: &'v CalcValue) -> Step<Item<'v>> {
    Ok(match value.core_ref() {
        CoreRef::Number(n) => Item::Number(site.argument(index, coerce::operand(n))?),
        CoreRef::Text(text) => Item::Text(text.utf16_code_units()),
        CoreRef::Logical(b) => Item::Logical(b),
        CoreRef::Error(code) => Item::Error(code),
        CoreRef::Empty | CoreRef::Missing => Item::Blank,
        CoreRef::Array(_) | CoreRef::Reference(_) => {
            return Err(site.not_modelled(Some(index)));
        }
    })
}

/// How a function family reads a blank or omitted lookup value.
#[derive(Clone, Copy, PartialEq, Eq)]
enum BlankKey {
    /// The MATCH family: as the number 0.
    Zero,
    /// XMATCH and XLOOKUP: as blank.
    Blank,
}

/// The lookup value, argument 0: its error is the result, an array is refused (Excel lifts the
/// function over it), and a blank or omitted value reads as `blank` says.
fn lookup_key<'v>(site: &Site<'_>, value: &'v CalcValue, blank: BlankKey) -> Step<Item<'v>> {
    match value.core_ref() {
        CoreRef::Error(code) => Err(Exit::Error(code)),
        CoreRef::Empty | CoreRef::Missing if blank == BlankKey::Zero => Ok(Item::Number(0.0)),
        _ => item(site, 0, value),
    }
}

/// The MATCH family's limit: a text lookup value of more than 255 UTF-16 code units is #VALUE!.
fn text_limit(key: Item<'_>) -> Step<()> {
    match key {
        Item::Text(units) if units.len() > 255 => Err(Exit::Error(WorksheetErrorCode::Value)),
        _ => Ok(()),
    }
}

/// Whether two items of one kind are equal: numbers by exact value, text through calc_func's
/// comparison (refused where it cannot decide), and two blanks always.
fn equal(site: &Site<'_>, a: Item<'_>, b: Item<'_>) -> Step<bool> {
    match (a, b) {
        (Item::Number(x), Item::Number(y)) => Ok(x == y),
        (Item::Text(x), Item::Text(y)) => comparison::texts(x, y, Want::Equality)
            .map(Ordering::is_eq)
            .map_err(|_| site.not_modelled(None)),
        (Item::Logical(x), Item::Logical(y)) => Ok(x == y),
        (Item::Error(x), Item::Error(y)) => Ok(x == y),
        (Item::Blank, Item::Blank) => Ok(true),
        _ => Ok(false),
    }
}

/// The place of an error among errors in XMATCH's order; `None` for the errors whose place is not
/// known.
fn error_rank(code: WorksheetErrorCode) -> Option<u8> {
    use WorksheetErrorCode::*;
    match code {
        Null => Some(0),
        Div0 => Some(1),
        Value => Some(2),
        Ref => Some(3),
        Name => Some(4),
        Num => Some(5),
        NA => Some(6),
        GettingData => Some(7),
        Spill => Some(8),
        Field => Some(9),
        Calc => Some(10),
        Busy | Blocked | Connect => None,
    }
}

/// XMATCH's order of two items: by kind, then within the kind. Text is ordered through
/// calc_func's comparison and refused where it cannot decide; two different errors are refused
/// when either has no known place.
fn order(site: &Site<'_>, a: Item<'_>, b: Item<'_>) -> Step<Ordering> {
    // total_cmp places -0 below +0, which `equal` holds equal: -0 is ordered as +0.
    let unsigned_zero = |n: f64| if n == 0.0 { 0.0 } else { n };
    match (a, b) {
        (Item::Number(x), Item::Number(y)) => Ok(unsigned_zero(x).total_cmp(&unsigned_zero(y))),
        (Item::Text(x), Item::Text(y)) => {
            comparison::texts(x, y, Want::Order).map_err(|_| site.not_modelled(None))
        }
        (Item::Logical(x), Item::Logical(y)) => Ok(x.cmp(&y)),
        (Item::Error(x), Item::Error(y)) if x == y => Ok(Ordering::Equal),
        (Item::Error(x), Item::Error(y)) => match (error_rank(x), error_rank(y)) {
            (Some(x), Some(y)) => Ok(x.cmp(&y)),
            _ => Err(site.not_modelled(None)),
        },
        (Item::Blank, Item::Blank) => Ok(Ordering::Equal),
        (a, b) => Ok(a.rank().cmp(&b.rank())),
    }
}

/// A text lookup value read as a pattern.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Token {
    /// This code unit, under the simple Unicode case fold.
    Unit(u16),
    /// `?`: one code unit.
    One,
    /// `*`: any run of code units.
    Run,
}

/// The pattern a text lookup value holds, or `None` when it holds no `*`, `?` or `~` and is
/// matched as plain text.
fn pattern(units: &[u16]) -> Option<Vec<Token>> {
    const STAR: u16 = b'*' as u16;
    const QUESTION: u16 = b'?' as u16;
    const TILDE: u16 = b'~' as u16;
    if !units
        .iter()
        .any(|&u| u == STAR || u == QUESTION || u == TILDE)
    {
        return None;
    }
    let mut tokens = Vec::with_capacity(units.len());
    let mut at = 0;
    while at < units.len() {
        match units[at] {
            TILDE => {
                // A tilde takes the next unit literally; one at the end is dropped.
                if let Some(&next) = units.get(at + 1) {
                    tokens.push(Token::Unit(next));
                }
                at += 2;
                continue;
            }
            STAR => tokens.push(Token::Run),
            QUESTION => tokens.push(Token::One),
            unit => tokens.push(Token::Unit(unit)),
        }
        at += 1;
    }
    Some(tokens)
}

/// Whether `text` matches `pattern`. Excel's fn1 Value2 controls use simple Unicode case
/// equality for literals, retain accents and count one UTF-16 unit for each `?`.
fn matches(_site: &Site<'_>, pattern: &[Token], text: &[u16]) -> Step<bool> {
    let fold = |unit: u16| {
        char::from_u32(u32::from(unit))
            .map(calc_value::unicode_simple_case_fold)
            .map_or(u32::from(unit), |ch| ch as u32)
    };
    let same = |p: u16, t: u16| p == t || fold(p) == fold(t);
    let (mut p, mut t) = (0, 0);
    let mut resume: Option<(usize, usize)> = None;
    while t < text.len() {
        match pattern.get(p) {
            Some(Token::Run) => {
                resume = Some((p, t));
                p += 1;
            }
            Some(Token::One) => {
                p += 1;
                t += 1;
            }
            Some(Token::Unit(unit)) if same(*unit, text[t]) => {
                p += 1;
                t += 1;
            }
            _ => match resume {
                Some((star, from)) => {
                    p = star + 1;
                    t = from + 1;
                    resume = Some((star, from + 1));
                }
                None => return Ok(false),
            },
        }
    }
    Ok(pattern[p..].iter().all(|token| *token == Token::Run))
}

/// What an exact search compares each cell with: the lookup value, or the pattern it holds.
enum Target<'k> {
    Value(Item<'k>),
    /// A text lookup value without wildcards in a search that reads them: equal text of the same
    /// length.
    SameLength(&'k [u16]),
    Pattern(Vec<Token>),
}

impl<'k> Target<'k> {
    /// The target of an exact search for `key`; `wildcards` reads a text key as a pattern, or
    /// asks for text of its length when it holds none.
    fn new(key: Item<'k>, wildcards: bool) -> Self {
        match key {
            Item::Text(units) if wildcards => match pattern(units) {
                Some(tokens) => Target::Pattern(tokens),
                None => Target::SameLength(units),
            },
            _ => Target::Value(key),
        }
    }

    fn hits(&self, site: &Site<'_>, cell: Item<'_>) -> Step<bool> {
        match (self, cell) {
            (Target::Value(key), cell) => equal(site, *key, cell),
            // Equality first: text the comparison cannot decide is refused whatever its length.
            (Target::SameLength(key), Item::Text(text)) => {
                Ok(equal(site, Item::Text(key), cell)? && key.len() == text.len())
            }
            (Target::SameLength(_), _) => Ok(false),
            (Target::Pattern(tokens), Item::Text(text)) => matches(site, tokens, text),
            (Target::Pattern(_), _) => Ok(false),
        }
    }
}

// ---------------------------------------------------------------------------------------------
// Lines: the row or column a lookup searches.

/// One row or one column of cells: part of a reference argument's area, or of an array argument,
/// or a single value.
#[derive(Clone, Copy)]
enum Line<'a> {
    /// A one-row or one-column area of the reference at argument `index`, read through the cell
    /// reader.
    Area { area: GridArea, index: usize },
    /// Row `fixed` (`along_row`) or column `fixed` of an array.
    Array {
        array: &'a CalcArray,
        along_row: bool,
        fixed: usize,
    },
    /// A single value, a line of one cell.
    One(&'a CalcValue),
}

impl Line<'_> {
    fn len(&self) -> u64 {
        match *self {
            Line::Area { area, .. } => area.cell_count(),
            Line::Array {
                array, along_row, ..
            } => {
                let shape = array.shape();
                (if along_row { shape.cols } else { shape.rows }) as u64
            }
            Line::One(_) => 1,
        }
    }

    /// Calls `visit` with each held (not blank) cell of the line, in order, with its position,
    /// until `visit` breaks. A reader that refuses the area, visits a cell outside it or visits
    /// cells out of order cannot supply the line: ReferenceUnreadable.
    fn scan(
        &self,
        site: &Site<'_>,
        services: &mut EvalServices<'_>,
        visit: &mut dyn FnMut(u64, &CalcValue) -> Step<ControlFlow<()>>,
    ) -> Step<()> {
        match *self {
            Line::Area { area, index } => {
                let first = area.first();
                let along_row = area.row_count() == 1;
                let cells = reader(site, services, index)?;
                let unreadable = || {
                    Exit::Fault(FunctionFault::ReferenceUnreadable {
                        id: site.id(),
                        index,
                    })
                };
                let mut stopped = Ok(());
                let mut next = 0u64;
                let read = cells.read_area(area, &mut |key: CellKey, value: &CalcValue| {
                    let step = if !area.contains(key) {
                        Err(unreadable())
                    } else {
                        let at = if along_row {
                            u64::from(key.column() - first.column())
                        } else {
                            u64::from(key.row() - first.row())
                        };
                        if at < next {
                            Err(unreadable())
                        } else {
                            next = at + 1;
                            if matches!(value.core_ref(), CoreRef::Empty | CoreRef::Missing) {
                                Ok(ControlFlow::Continue(()))
                            } else {
                                visit(at, value)
                            }
                        }
                    };
                    match step {
                        Ok(flow) => flow,
                        Err(exit) => {
                            stopped = Err(exit);
                            ControlFlow::Break(())
                        }
                    }
                });
                read.map_err(|Unreadable| unreadable())?;
                stopped
            }
            Line::Array {
                array,
                along_row,
                fixed,
            } => {
                for at in 0..self.len() as usize {
                    let (row, column) = if along_row { (fixed, at) } else { (at, fixed) };
                    let value = array.get(row, column).expect("a cell of the array");
                    if matches!(value.core_ref(), CoreRef::Empty | CoreRef::Missing) {
                        continue;
                    }
                    if visit(at as u64, value)?.is_break() {
                        break;
                    }
                }
                Ok(())
            }
            Line::One(value) => {
                if !matches!(value.core_ref(), CoreRef::Empty | CoreRef::Missing) {
                    // The only cell: whether the visit breaks changes nothing.
                    let _ = visit(0, value)?;
                }
                Ok(())
            }
        }
    }

    /// The held cells of the line, in order.
    fn held(&self, site: &Site<'_>, services: &mut EvalServices<'_>) -> Step<Held> {
        let mut cells = Vec::new();
        self.scan(site, services, &mut |at, value| {
            cells.push((at, value.clone()));
            Ok(ControlFlow::Continue(()))
        })?;
        Ok(Held {
            len: self.len(),
            cells,
        })
    }
}

/// The held cells of a line by position, in increasing order; every other position is blank.
struct Held {
    len: u64,
    cells: Vec<(u64, CalcValue)>,
}

impl Held {
    /// The value at `at`, which is below `len`: the held cell's, or Empty.
    fn value(&self, at: u64) -> Option<&CalcValue> {
        self.cells
            .binary_search_by(|(held, _)| held.cmp(&at))
            .ok()
            .map(|found| &self.cells[found].1)
    }

    /// The first (`from_end` false) or last blank position, if any.
    fn blank(&self, from_end: bool) -> Option<u64> {
        if from_end {
            let mut expected = self.len;
            for (at, _) in self.cells.iter().rev() {
                if at + 1 != expected {
                    return Some(expected - 1);
                }
                expected = *at;
            }
            expected.checked_sub(1)
        } else {
            let mut expected = 0;
            for (at, _) in &self.cells {
                if *at != expected {
                    return Some(expected);
                }
                expected = at + 1;
            }
            (expected < self.len).then_some(expected)
        }
    }
}

/// The services' cell reader, which a kernel needs to read the reference at argument `index`.
fn reader<'a, 's>(
    site: &Site<'_>,
    services: &'a mut EvalServices<'s>,
    index: usize,
) -> Step<&'a mut (dyn CellReader + 's)> {
    services
        .cells()
        .ok_or(Exit::Fault(FunctionFault::ReferenceUnreadable {
            id: site.id(),
            index,
        }))
}

/// The value of one cell of the reference at argument `index`: the held value, or Empty for a
/// blank cell.
fn cell_value(
    site: &Site<'_>,
    services: &mut EvalServices<'_>,
    index: usize,
    key: CellKey,
) -> Step<CalcValue> {
    let mut found = CalcValue::empty();
    Line::Area {
        area: GridArea::cell(key),
        index,
    }
    .scan(site, services, &mut |_, value| {
        found = value.clone();
        Ok(ControlFlow::Break(()))
    })?;
    Ok(found)
}

/// A rectangle of cells a lookup reads lines from: a reference's single area, an array, or a
/// single value.
#[derive(Clone, Copy)]
enum Grid<'a> {
    Area { area: GridArea, index: usize },
    Array(&'a CalcArray),
    One(&'a CalcValue),
}

impl<'a> Grid<'a> {
    fn rows(&self) -> u64 {
        match self {
            Grid::Area { area, .. } => u64::from(area.row_count()),
            Grid::Array(array) => array.shape().rows as u64,
            Grid::One(_) => 1,
        }
    }

    fn columns(&self) -> u64 {
        match self {
            Grid::Area { area, .. } => u64::from(area.column_count()),
            Grid::Array(array) => array.shape().cols as u64,
            Grid::One(_) => 1,
        }
    }

    /// Row `row` (0-based) as a line.
    fn row_line(&self, row: u64) -> Line<'a> {
        match *self {
            Grid::Area { area, index } => {
                let first = area.first();
                let row = first.row() + row as u32;
                Line::Area {
                    area: span(area, (row, first.column()), (row, area.last().column())),
                    index,
                }
            }
            Grid::Array(array) => Line::Array {
                array,
                along_row: true,
                fixed: row as usize,
            },
            Grid::One(value) => Line::One(value),
        }
    }

    /// Column `column` (0-based) as a line.
    fn column_line(&self, column: u64) -> Line<'a> {
        match *self {
            Grid::Area { area, index } => {
                let first = area.first();
                let column = first.column() + column as u32;
                Line::Area {
                    area: span(area, (first.row(), column), (area.last().row(), column)),
                    index,
                }
            }
            Grid::Array(array) => Line::Array {
                array,
                along_row: false,
                fixed: column as usize,
            },
            Grid::One(value) => Line::One(value),
        }
    }

    /// The grid as one line, when it has one row or one column.
    fn line(&self) -> Option<Line<'a>> {
        if self.rows() == 1 {
            Some(self.row_line(0))
        } else if self.columns() == 1 {
            Some(self.column_line(0))
        } else {
            None
        }
    }

    /// The value of the cell at `row` and `column` (0-based): read through the cell reader for a
    /// reference, Empty for a blank cell.
    fn value(
        &self,
        site: &Site<'_>,
        services: &mut EvalServices<'_>,
        row: u64,
        column: u64,
    ) -> Step<CalcValue> {
        match *self {
            Grid::Area { area, index } => {
                let first = area.first();
                let key = CellKey::new(
                    area.sheet(),
                    first.row() + row as u32,
                    first.column() + column as u32,
                )
                .expect("a cell of the area");
                cell_value(site, services, index, key)
            }
            Grid::Array(array) => {
                let value = array
                    .get(row as usize, column as usize)
                    .expect("a cell of the array");
                Ok(match value.core_ref() {
                    CoreRef::Missing => CalcValue::empty(),
                    _ => value.clone(),
                })
            }
            Grid::One(value) => Ok(value.clone()),
        }
    }
}

/// The area of `area`'s sheet from `(row, column)` to `(row, column)`, both inside `area`.
fn span(area: GridArea, first: (u32, u32), last: (u32, u32)) -> GridArea {
    let key = |(row, column)| CellKey::new(area.sheet(), row, column).expect("a cell of the area");
    GridArea::new(key(first), key(last)).expect("corners of one sheet")
}

/// What a lookup argument at `index` is as a grid, or why it is none.
enum Shape<'a> {
    Grid(Grid<'a>),
    /// An error value: the call's result.
    Error(WorksheetErrorCode),
    /// A reference of several areas.
    Areas,
    /// Blank or omitted.
    Blank,
}

fn shape(value: &CalcValue, index: usize) -> Shape<'_> {
    match value.core_ref() {
        CoreRef::Reference(reference) => match reference.areas() {
            [area] => Shape::Grid(Grid::Area { area: *area, index }),
            _ => Shape::Areas,
        },
        CoreRef::Array(array) => Shape::Grid(Grid::Array(array)),
        CoreRef::Error(code) => Shape::Error(code),
        CoreRef::Empty | CoreRef::Missing => Shape::Blank,
        CoreRef::Number(_) | CoreRef::Text(_) | CoreRef::Logical(_) => {
            Shape::Grid(Grid::One(value))
        }
    }
}

// ---------------------------------------------------------------------------------------------
// Searches.

/// The position of the first cell of `line` that `target` hits, in a walk from the start; the
/// walk stops there.
fn first_hit(
    site: &Site<'_>,
    services: &mut EvalServices<'_>,
    line: Line<'_>,
    target: &Target<'_>,
) -> Step<Option<u64>> {
    let mut found = None;
    line.scan(site, services, &mut |at, value| {
        if target.hits(site, item(site, 1, value)?)? {
            found = Some(at);
            return Ok(ControlFlow::Break(()));
        }
        Ok(ControlFlow::Continue(()))
    })?;
    Ok(found)
}

/// MATCH's bisection (match_type 1, and VLOOKUP, HLOOKUP and LOOKUP in approximate mode): see the
/// module documentation.
fn bisect(site: &Site<'_>, held: &Held, key: Item<'_>) -> Step<Option<u64>> {
    let mut kind = Vec::new();
    for (at, value) in &held.cells {
        let cell = item(site, 1, value)?;
        if cell.rank() == key.rank() {
            kind.push((*at, value));
        }
    }
    let (mut lo, mut hi) = (0i64, held.len as i64 - 1);
    let mut best = None;
    while lo <= hi {
        let mid = (lo + hi) / 2;
        let k = kind.partition_point(|(at, _)| (*at as i64) < mid);
        let Some(&(at, value)) = kind.get(k).filter(|(at, _)| (*at as i64) <= hi) else {
            hi = mid - 1;
            continue;
        };
        match order(site, item(site, 1, value)?, key)? {
            Ordering::Equal => {
                let mut last = k;
                while let Some(&(next, value)) = kind.get(last + 1) {
                    if next != kind[last].0 + 1
                        || (next as i64) > hi
                        || !equal(site, item(site, 1, value)?, key)?
                    {
                        break;
                    }
                    last += 1;
                }
                return Ok(Some(kind[last].0));
            }
            Ordering::Less => {
                best = Some(at);
                lo = at as i64 + 1;
            }
            Ordering::Greater => hi = at as i64 - 1,
        }
    }
    Ok(best)
}

/// MATCH's walk for match_type -1: see the module documentation.
fn walk_descending(
    site: &Site<'_>,
    services: &mut EvalServices<'_>,
    line: Line<'_>,
    key: Item<'_>,
) -> Step<Option<u64>> {
    let mut best = None;
    line.scan(site, services, &mut |at, value| {
        let cell = item(site, 1, value)?;
        if cell.rank() != key.rank() {
            return Ok(ControlFlow::Continue(()));
        }
        Ok(match order(site, cell, key)? {
            Ordering::Equal => {
                best = Some(at);
                ControlFlow::Break(())
            }
            Ordering::Greater => {
                best = Some(at);
                ControlFlow::Continue(())
            }
            Ordering::Less => ControlFlow::Break(()),
        })
    })?;
    // An equal cell ends the walk as the result; a smaller one ends it with the last larger.
    Ok(best)
}

/// XMATCH's match_mode.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Mode {
    Exact,
    NextSmaller,
    NextLarger,
    Wildcard,
}

/// XMATCH's search_mode.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Search {
    FirstToLast,
    LastToFirst,
    Ascending,
    Descending,
}

/// XMATCH's linear search (search_mode 1 or -1).
fn xmatch_linear(
    site: &Site<'_>,
    services: &mut EvalServices<'_>,
    line: Line<'_>,
    key: Item<'_>,
    mode: Mode,
    from_end: bool,
) -> Step<Option<u64>> {
    if !from_end && matches!(mode, Mode::Exact | Mode::Wildcard) && !matches!(key, Item::Blank) {
        // A walk from the start may stop at the first hit.
        return first_hit(
            site,
            services,
            line,
            &Target::new(key, mode == Mode::Wildcard),
        );
    }
    let held = line.held(site, services)?;
    if let Item::Blank = key {
        // A blank lookup value matches only blank cells, the first or last of them; with none,
        // the next smaller value is the largest other value, and nothing is larger.
        if let Some(at) = held.blank(from_end) {
            return Ok(Some(at));
        }
        if mode != Mode::NextSmaller {
            return Ok(None);
        }
    }
    let target = Target::new(key, mode == Mode::Wildcard);
    let in_search_order = || -> Box<dyn Iterator<Item = &(u64, CalcValue)>> {
        if from_end {
            Box::new(held.cells.iter().rev())
        } else {
            Box::new(held.cells.iter())
        }
    };
    // An equal cell is the result, the first met: no order is needed when there is one.
    for (at, value) in in_search_order() {
        if target.hits(site, item(site, 1, value)?)? {
            return Ok(Some(*at));
        }
    }
    if matches!(mode, Mode::Exact | Mode::Wildcard) {
        return Ok(None);
    }
    let wanted = if mode == Mode::NextSmaller {
        Ordering::Less
    } else {
        Ordering::Greater
    };
    // The candidates of the kind nearest the lookup value, in the order met: the kinds decide
    // first, so two candidates are ordered within their kind only when that kind holds the result
    // (an order calc_func's comparison cannot decide between two texts refuses only then).
    let mut nearest_kind: Vec<(u64, Item<'_>)> = Vec::new();
    for (at, value) in in_search_order() {
        let cell = item(site, 1, value)?;
        if order(site, cell, key)? != wanted {
            continue;
        }
        match nearest_kind.first() {
            Some((_, kept)) if kept.rank() == cell.rank() => nearest_kind.push((*at, cell)),
            Some((_, kept)) if cell.rank().cmp(&kept.rank()) != wanted.reverse() => {}
            _ => {
                nearest_kind.clear();
                nearest_kind.push((*at, cell));
            }
        }
    }
    let mut best: Option<(u64, Item<'_>)> = None;
    for (at, cell) in nearest_kind {
        let nearer = match best {
            None => true,
            // A strictly nearer candidate replaces the best, so ties keep the first met.
            Some((_, kept)) => order(site, cell, kept)? == wanted.reverse(),
        };
        if nearer {
            best = Some((at, cell));
        }
    }
    // Blank cells sort after every other value: they are the next larger value when no other
    // value is larger.
    match best {
        Some((at, _)) => Ok(Some(at)),
        None if mode == Mode::NextLarger => Ok(held.blank(from_end)),
        None => Ok(None),
    }
}

/// XMATCH's binary search (search_mode 2 or -2): see the module documentation.
fn xmatch_binary(
    site: &Site<'_>,
    held: &Held,
    key: Item<'_>,
    mode: Mode,
    descending: bool,
) -> Step<Option<u64>> {
    let (mut lo, mut hi) = (0i64, held.len as i64 - 1);
    let (mut exact, mut candidate) = (None, None);
    while lo <= hi {
        let mid = (lo + hi) / 2;
        let cell = match held.value(mid as u64) {
            Some(value) => item(site, 1, value)?,
            None => Item::Blank,
        };
        let go_left = if cell.rank() == key.rank() && equal(site, cell, key)? {
            exact = Some(mid as u64);
            !descending
        } else if order(site, cell, key)? == Ordering::Less {
            if mode == Mode::NextSmaller {
                candidate = Some(mid as u64);
            }
            descending
        } else {
            if mode == Mode::NextLarger {
                candidate = Some(mid as u64);
            }
            !descending
        };
        if go_left {
            hi = mid - 1;
        } else {
            lo = mid + 1;
        }
    }
    Ok(exact.or(candidate))
}

/// An index argument at `index` read as a number: worksheet number coercion, its error the
/// result.
fn index_number(site: &Site<'_>, index: usize, value: &CalcValue) -> Step<f64> {
    site.number_argument(index, value)
}

/// XMATCH's and XLOOKUP's match_mode and search_mode, each read as a number and rounded down.
/// An unknown mode, and wildcards with a binary search, are #VALUE!; regex matching
/// (match_mode 3) is not modelled.
fn modes(
    site: &Site<'_>,
    match_mode: Option<(usize, &CalcValue)>,
    search_mode: Option<(usize, &CalcValue)>,
) -> Step<(Mode, Search)> {
    let read = |argument: Option<(usize, &CalcValue)>, default: f64| -> Step<f64> {
        match argument {
            None => Ok(default),
            Some((_, value)) if value.is_missing() || value.is_empty() => Ok(0.0),
            Some((index, value)) => index_number(site, index, value).map(f64::floor),
        }
    };
    let mode_number = read(match_mode, 0.0)?;
    let search_number = match search_mode {
        Some((_, value)) if value.is_missing() => 1.0,
        _ => read(search_mode, 1.0)?,
    };
    let invalid = || Exit::Error(WorksheetErrorCode::Value);
    let mode = match mode_number {
        0.0 => Mode::Exact,
        -1.0 => Mode::NextSmaller,
        1.0 => Mode::NextLarger,
        2.0 => Mode::Wildcard,
        3.0 => return Err(site.not_modelled(match_mode.map(|(index, _)| index))),
        _ => return Err(invalid()),
    };
    let search = match search_number {
        1.0 => Search::FirstToLast,
        -1.0 => Search::LastToFirst,
        2.0 => Search::Ascending,
        -2.0 => Search::Descending,
        _ => return Err(invalid()),
    };
    if mode == Mode::Wildcard && matches!(search, Search::Ascending | Search::Descending) {
        return Err(invalid());
    }
    Ok((mode, search))
}

/// XMATCH's search of `line` for `key`.
fn xsearch(
    site: &Site<'_>,
    services: &mut EvalServices<'_>,
    line: Line<'_>,
    key: Item<'_>,
    mode: Mode,
    search: Search,
) -> Step<Option<u64>> {
    match search {
        Search::FirstToLast => xmatch_linear(site, services, line, key, mode, false),
        Search::LastToFirst => xmatch_linear(site, services, line, key, mode, true),
        Search::Ascending | Search::Descending => {
            let held = line.held(site, services)?;
            xmatch_binary(site, &held, key, mode, search == Search::Descending)
        }
    }
}

/// A 1-based position as the number a function returns.
fn position(site: &Site<'_>, at: u64) -> Step<CalcValue> {
    // A position is below 2^34, which binary64 holds exactly.
    site.number((at + 1) as f64)
}

// ---------------------------------------------------------------------------------------------
// MATCH, XMATCH.

/// MATCH(lookup_value, lookup_array, [match_type]): the 1-based position of the lookup value in a
/// row or a column. match_type is read as a number and only its sign counts: 0 an exact match
/// (with wildcards), positive (the default) the bisection, negative the descending walk; an
/// omitted or blank match_type is 0. A lookup array that is not one row or one column, a
/// reference of several areas, and a lookup array that is a single value and not a reference are
/// #N/A, as is no match.
fn match_(
    arguments: &[CalcValue],
    services: &mut EvalServices<'_>,
) -> Result<CalcValue, FunctionFault> {
    let site = Site::new(MATCH, services);
    Site::finish(matched(&site, arguments, services))
}

fn matched(
    site: &Site<'_>,
    arguments: &[CalcValue],
    services: &mut EvalServices<'_>,
) -> Step<CalcValue> {
    let key = lookup_key(site, &arguments[0], BlankKey::Zero)?;
    let grid = match shape(&arguments[1], 1) {
        Shape::Error(code) => return Err(Exit::Error(code)),
        Shape::Grid(grid) => Some(grid),
        Shape::Areas | Shape::Blank => None,
    };
    let sign = match arguments.get(2) {
        None => 1.0,
        Some(value) if value.is_missing() || value.is_empty() => 0.0,
        Some(value) => index_number(site, 2, value)?,
    };
    text_limit(key)?;
    let not_found = Exit::Error(WorksheetErrorCode::NA);
    let line = match grid {
        // A single value given directly is not searched.
        Some(Grid::One(_)) | None => return Err(not_found),
        Some(grid) => grid.line().ok_or(not_found)?,
    };
    let found = if sign == 0.0 {
        first_hit(site, services, line, &Target::new(key, true))?
    } else if sign > 0.0 {
        bisect(site, &line.held(site, services)?, key)?
    } else {
        walk_descending(site, services, line, key)?
    };
    match found {
        Some(at) => position(site, at),
        None => Err(Exit::Error(WorksheetErrorCode::NA)),
    }
}

/// XMATCH(lookup_value, lookup_array, [match_mode], [search_mode]): the 1-based position, by the
/// modes the module documentation describes. A lookup array of more than one row and column is
/// #VALUE!, a reference of several areas #N/A; a single value is a line of one cell. A blank or
/// omitted lookup array is #VALUE! in its argument's turn, before the modes are read.
fn xmatch(
    arguments: &[CalcValue],
    services: &mut EvalServices<'_>,
) -> Result<CalcValue, FunctionFault> {
    let site = Site::new(XMATCH, services);
    Site::finish(xmatched(&site, arguments, services))
}

fn xmatched(
    site: &Site<'_>,
    arguments: &[CalcValue],
    services: &mut EvalServices<'_>,
) -> Step<CalcValue> {
    let key = lookup_key(site, &arguments[0], BlankKey::Blank)?;
    let grid = match shape(&arguments[1], 1) {
        Shape::Error(code) => return Err(Exit::Error(code)),
        Shape::Areas => None,
        Shape::Blank => return Err(Exit::Error(WorksheetErrorCode::Value)),
        Shape::Grid(grid) => Some(grid),
    };
    let (mode, search) = modes(
        site,
        arguments.get(2).map(|value| (2, value)),
        arguments.get(3).map(|value| (3, value)),
    )?;
    let Some(grid) = grid else {
        return Err(Exit::Error(WorksheetErrorCode::NA));
    };
    let line = grid.line().ok_or(Exit::Error(WorksheetErrorCode::Value))?;
    match xsearch(site, services, line, key, mode, search)? {
        Some(at) => position(site, at),
        None => Err(Exit::Error(WorksheetErrorCode::NA)),
    }
}

// ---------------------------------------------------------------------------------------------
// XLOOKUP.

/// XLOOKUP(lookup_value, lookup_array, return_array, [if_not_found], [match_mode],
/// [search_mode]): XMATCH's search of lookup_array, and the matching row (a vertical lookup
/// array) or column (a horizontal one) of return_array, which must have as many rows or columns:
/// a reference into return_array when it is one, else the array's part. With no match the result
/// is if_not_found as it is, or #N/A when it is omitted. A lookup array of one cell selects the
/// whole of a return array of one row, else of one column; a return array of more rows and columns
/// is #VALUE! for it. A blank or omitted lookup or return array is #VALUE! in its argument's turn,
/// before the modes are read; the shape rules apply after them.
fn xlookup(
    arguments: &[CalcValue],
    services: &mut EvalServices<'_>,
) -> Result<CalcValue, FunctionFault> {
    let site = Site::new(XLOOKUP, services);
    Site::finish(xlooked_up(&site, arguments, services))
}

fn xlooked_up(
    site: &Site<'_>,
    arguments: &[CalcValue],
    services: &mut EvalServices<'_>,
) -> Step<CalcValue> {
    let key = lookup_key(site, &arguments[0], BlankKey::Blank)?;
    let lookup_grid = match shape(&arguments[1], 1) {
        Shape::Error(code) => return Err(Exit::Error(code)),
        Shape::Areas => None,
        Shape::Blank => return Err(Exit::Error(WorksheetErrorCode::Value)),
        Shape::Grid(grid) => Some(grid),
    };
    let return_grid = match shape(&arguments[2], 2) {
        Shape::Error(code) => return Err(Exit::Error(code)),
        Shape::Areas => None,
        Shape::Blank => return Err(Exit::Error(WorksheetErrorCode::Value)),
        Shape::Grid(grid) => Some(grid),
    };
    let (mode, search) = modes(
        site,
        arguments.get(4).map(|value| (4, value)),
        arguments.get(5).map(|value| (5, value)),
    )?;
    let (Some(lookup_grid), Some(return_grid)) = (lookup_grid, return_grid) else {
        return Err(Exit::Error(WorksheetErrorCode::NA));
    };
    let value_error = Exit::Error(WorksheetErrorCode::Value);
    let line = lookup_grid
        .line()
        .ok_or(Exit::Error(WorksheetErrorCode::Value))?;
    let (rows, columns) = (lookup_grid.rows(), lookup_grid.columns());
    // Whether the result is a row of the return array (true) or a column.
    let by_row = if rows == 1 && columns == 1 {
        if return_grid.rows() == 1 {
            true
        } else if return_grid.columns() == 1 {
            false
        } else {
            return Err(value_error);
        }
    } else if columns == 1 {
        if return_grid.rows() != rows {
            return Err(value_error);
        }
        true
    } else {
        if return_grid.columns() != columns {
            return Err(value_error);
        }
        false
    };
    let Some(at) = xsearch(site, services, line, key, mode, search)? else {
        return match arguments.get(3) {
            Some(fallback) if !fallback.is_missing() => Ok(fallback.clone()),
            _ => Err(Exit::Error(WorksheetErrorCode::NA)),
        };
    };
    Ok(part(&arguments[2], return_grid, by_row, at))
}

/// Row (`by_row`) or column `at` (0-based) of a return grid: a reference into its area, or the
/// array's part, or the single value.
fn part(original: &CalcValue, grid: Grid<'_>, by_row: bool, at: u64) -> CalcValue {
    match grid {
        Grid::Area { area, .. } => {
            let first = area.first();
            let last = area.last();
            let selected = if by_row {
                let row = first.row() + at as u32;
                span(area, (row, first.column()), (row, last.column()))
            } else {
                let column = first.column() + at as u32;
                span(area, (first.row(), column), (last.row(), column))
            };
            CalcValue::reference(ReferenceLike::area(selected))
        }
        Grid::Array(array) => {
            let shape = array.shape();
            let cells: Vec<CalcValue> = if by_row {
                array
                    .row_slice(at as usize)
                    .expect("a row of the array")
                    .to_vec()
            } else {
                (0..shape.rows)
                    .map(|row| {
                        array
                            .get(row, at as usize)
                            .expect("a cell of the array")
                            .clone()
                    })
                    .collect()
            };
            if cells.len() == 1 {
                return cells.into_iter().next().expect("one cell");
            }
            let shape = if by_row {
                ArrayShape {
                    rows: 1,
                    cols: cells.len(),
                }
            } else {
                ArrayShape {
                    rows: cells.len(),
                    cols: 1,
                }
            };
            CalcValue::array(CalcArray::new(shape, cells).expect("a row or column"))
        }
        Grid::One(_) => original.clone(),
    }
}

// ---------------------------------------------------------------------------------------------
// VLOOKUP, HLOOKUP.

/// VLOOKUP(lookup_value, table_array, col_index_num, [range_lookup]): the value in column
/// col_index_num of the row whose first cell matches; HLOOKUP is the same with rows and columns
/// exchanged. range_lookup is read as a logical; omitted, it is TRUE (the bisection), and blank or
/// given empty, FALSE (an exact match, with wildcards). The index is read as a number and
/// truncated, and checked only after a match: below 1 is #VALUE! (from -2^31 upward), above the
/// table's width (or at or below -2^31) #REF!. A table of several areas, a single value that is
/// not a reference, a blank or omitted table, and no match are #N/A. A blank result cell is Empty.
fn vlookup(
    arguments: &[CalcValue],
    services: &mut EvalServices<'_>,
) -> Result<CalcValue, FunctionFault> {
    let site = Site::new(VLOOKUP, services);
    Site::finish(table_lookup(&site, arguments, services, true))
}

fn hlookup(
    arguments: &[CalcValue],
    services: &mut EvalServices<'_>,
) -> Result<CalcValue, FunctionFault> {
    let site = Site::new(HLOOKUP, services);
    Site::finish(table_lookup(&site, arguments, services, false))
}

fn table_lookup(
    site: &Site<'_>,
    arguments: &[CalcValue],
    services: &mut EvalServices<'_>,
    vertical: bool,
) -> Step<CalcValue> {
    let key = lookup_key(site, &arguments[0], BlankKey::Zero)?;
    let grid = match shape(&arguments[1], 1) {
        Shape::Error(code) => return Err(Exit::Error(code)),
        Shape::Grid(Grid::One(_)) | Shape::Areas | Shape::Blank => None,
        Shape::Grid(grid) => Some(grid),
    };
    let chosen = index_number(site, 2, &arguments[2])?.trunc();
    let approximate = match arguments.get(3) {
        None => true,
        Some(value) => site.argument(3, coerce::logical(value))?,
    };
    text_limit(key)?;
    let Some(grid) = grid else {
        return Err(Exit::Error(WorksheetErrorCode::NA));
    };
    let line = if vertical {
        grid.column_line(0)
    } else {
        grid.row_line(0)
    };
    let found = if approximate {
        bisect(site, &line.held(site, services)?, key)?
    } else {
        first_hit(site, services, line, &Target::new(key, true))?
    };
    let Some(at) = found else {
        return Err(Exit::Error(WorksheetErrorCode::NA));
    };
    let width = if vertical {
        grid.columns()
    } else {
        grid.rows()
    };
    if chosen <= -2_147_483_648.0 || chosen > width as f64 {
        return Err(Exit::Error(WorksheetErrorCode::Ref));
    }
    if chosen < 1.0 {
        return Err(Exit::Error(WorksheetErrorCode::Value));
    }
    let other = chosen as u64 - 1;
    if vertical {
        grid.value(site, services, at, other)
    } else {
        grid.value(site, services, other, at)
    }
}

// ---------------------------------------------------------------------------------------------
// LOOKUP.

/// LOOKUP(lookup_value, lookup_vector, [result_vector]): the bisection of the lookup line, and
/// the value at the found position of the result line. With two arguments a grid wider than it is
/// tall is searched in its first row and answered from its last row, any other in its first
/// column and answered from its last column; a third argument gives the result line, which must be
/// one row or one column (a single cell runs along its row). A result line shorter than the found
/// position is #N/A for an array; for a reference Excel reads past it, which is not modelled. A
/// lookup or result reference of several areas, and no match, are #N/A; a blank or omitted lookup
/// or result vector is #VALUE! in its argument's turn. A blank result cell is Empty.
fn lookup(
    arguments: &[CalcValue],
    services: &mut EvalServices<'_>,
) -> Result<CalcValue, FunctionFault> {
    let site = Site::new(LOOKUP, services);
    Site::finish(looked_up(&site, arguments, services))
}

fn looked_up(
    site: &Site<'_>,
    arguments: &[CalcValue],
    services: &mut EvalServices<'_>,
) -> Step<CalcValue> {
    let key = lookup_key(site, &arguments[0], BlankKey::Zero)?;
    let grid = match shape(&arguments[1], 1) {
        Shape::Error(code) => return Err(Exit::Error(code)),
        Shape::Areas => None,
        Shape::Blank => return Err(Exit::Error(WorksheetErrorCode::Value)),
        Shape::Grid(grid) => Some(grid),
    };
    let result = match arguments.get(2).map(|value| shape(value, 2)) {
        None => None,
        Some(Shape::Error(code)) => return Err(Exit::Error(code)),
        Some(Shape::Areas) => return Err(Exit::Error(WorksheetErrorCode::NA)),
        Some(Shape::Blank) => return Err(Exit::Error(WorksheetErrorCode::Value)),
        Some(Shape::Grid(result)) => Some(result),
    };
    text_limit(key)?;
    let Some(grid) = grid else {
        return Err(Exit::Error(WorksheetErrorCode::NA));
    };
    let wide = grid.columns() > grid.rows();
    let line = if wide {
        grid.row_line(0)
    } else {
        grid.column_line(0)
    };
    let Some(at) = bisect(site, &line.held(site, services)?, key)? else {
        return Err(Exit::Error(WorksheetErrorCode::NA));
    };
    let Some(result) = result else {
        return if wide {
            grid.value(site, services, grid.rows() - 1, at)
        } else {
            grid.value(site, services, at, grid.columns() - 1)
        };
    };
    let (rows, columns) = (result.rows(), result.columns());
    let along_row = if rows == 1 {
        true
    } else if columns == 1 {
        false
    } else {
        return Err(Exit::Error(WorksheetErrorCode::NA));
    };
    let length = if along_row { columns } else { rows };
    if at >= length {
        return match result {
            // Excel reads the cells past the reference given, which the engine cannot see as
            // prerequisites of the formula: not modelled.
            Grid::Area { .. } => Err(site.not_modelled(Some(2))),
            Grid::Array(_) | Grid::One(_) => Err(Exit::Error(WorksheetErrorCode::NA)),
        };
    }
    if along_row {
        result.value(site, services, 0, at)
    } else {
        result.value(site, services, at, 0)
    }
}

// ---------------------------------------------------------------------------------------------
// INDEX.

/// INDEX(array, row_num, [column_num], [area_num]). Each index is read as a number; a negative
/// one is #VALUE!, a fraction is truncated, and an omitted or blank row_num or column_num is 0,
/// which selects every row or column. area_num, 1 when omitted, picks an area of a reference of
/// several areas; below 1 (an empty area_num included) it is #VALUE!, beyond the areas (or above 1
/// for an array) #REF!. With column_num omitted, row_num indexes the columns of a one-row area or
/// array and the rows of a one-column one; for a reference of more rows and columns it is #REF!,
/// for such an array it selects a whole row. An index beyond the area or array is #REF!. A blank or
/// omitted first argument is #VALUE! before the other arguments are read; then the argument errors
/// come, in order, then the #VALUE! rules, then the #REF! rules.
///
/// A reference gives a reference to the cell, row, column or area selected, so INDEX's result
/// stays a reference; an array gives the element, or the array of the row or column selected; a
/// single value is a 1-by-1 array whose every selection the bounds admit gives the value itself.
fn index(
    arguments: &[CalcValue],
    services: &mut EvalServices<'_>,
) -> Result<CalcValue, FunctionFault> {
    let site = Site::new(INDEX, services);
    Site::finish(indexed(&site, arguments))
}

/// An index argument: the number it gives, 0 when omitted or blank.
fn selector(site: &Site<'_>, index: usize, value: Option<&CalcValue>) -> Step<f64> {
    match value {
        None => Ok(0.0),
        Some(value) if value.is_missing() || value.is_empty() => Ok(0.0),
        Some(value) => index_number(site, index, value),
    }
}

fn indexed(site: &Site<'_>, arguments: &[CalcValue]) -> Step<CalcValue> {
    let source = &arguments[0];
    match source.core_ref() {
        CoreRef::Error(code) => return Err(Exit::Error(code)),
        CoreRef::Empty | CoreRef::Missing => {
            return Err(Exit::Error(WorksheetErrorCode::Value));
        }
        _ => {}
    }
    let row = selector(site, 1, arguments.get(1))?;
    let column_omitted = arguments.len() < 3;
    let column = selector(site, 2, arguments.get(2))?;
    let area_number = match arguments.get(3) {
        None => 1.0,
        Some(value) if value.is_missing() || value.is_empty() => 0.0,
        Some(value) => index_number(site, 3, value)?,
    };
    let value_error = Exit::Error(WorksheetErrorCode::Value);
    let ref_error = || Exit::Error(WorksheetErrorCode::Ref);
    if row < 0.0 || column < 0.0 || area_number.trunc() < 1.0 {
        return Err(value_error);
    }
    let (row, column, area_number) = (row.trunc(), column.trunc(), area_number.trunc());
    let grid = match source.core_ref() {
        CoreRef::Reference(reference) => {
            let areas = reference.areas();
            if area_number > areas.len() as f64 {
                return Err(ref_error());
            }
            Grid::Area {
                area: areas[area_number as usize - 1],
                index: 0,
            }
        }
        CoreRef::Array(array) => {
            if area_number > 1.0 {
                return Err(ref_error());
            }
            Grid::Array(array)
        }
        _ => {
            if area_number > 1.0 {
                return Err(ref_error());
            }
            Grid::One(source)
        }
    };
    let (rows, columns) = (grid.rows() as f64, grid.columns() as f64);
    let (row, column) = if column_omitted {
        if rows == 1.0 {
            (1.0, row)
        } else if columns == 1.0 {
            (row, 1.0)
        } else if matches!(grid, Grid::Area { .. }) {
            return Err(ref_error());
        } else {
            (row, 0.0)
        }
    } else {
        (row, column)
    };
    if row > rows || column > columns {
        return Err(ref_error());
    }
    let (row, column) = (row as u64, column as u64);
    Ok(match grid {
        Grid::Area { area, .. } => {
            let first = area.first();
            let last = area.last();
            let (top, bottom) = match row {
                0 => (first.row(), last.row()),
                n => (first.row() + n as u32 - 1, first.row() + n as u32 - 1),
            };
            let (left, right) = match column {
                0 => (first.column(), last.column()),
                n => (first.column() + n as u32 - 1, first.column() + n as u32 - 1),
            };
            CalcValue::reference(ReferenceLike::area(span(
                area,
                (top, left),
                (bottom, right),
            )))
        }
        Grid::Array(array) => select(array, row, column),
        // The bounds admit only the value itself.
        Grid::One(value) => value.clone(),
    })
}

/// The element at 1-based `row` and `column` of an array, or, where either is 0, the array of
/// every row or column it selects.
fn select(array: &CalcArray, row: u64, column: u64) -> CalcValue {
    let shape = array.shape();
    if row > 0 && column > 0 {
        let value = array
            .get(row as usize - 1, column as usize - 1)
            .expect("an element of the array");
        return match value.core_ref() {
            CoreRef::Missing => CalcValue::empty(),
            // An array a cell of the array holds (a GROUPBY result's, dc-h9u) is given boxed:
            // Excel 16.0.20527 shows `=INDEX(GROUPBY(k,v,LAMBDA(x,x)),1,2)` as the group's first
            // value and counts the whole group in ROWS and SUM of it.
            CoreRef::Array(held) if value.callable_value().is_none() => {
                CalcValue::array(held.clone().boxed())
            }
            _ => value.clone(),
        };
    }
    let rows: Vec<usize> = match row {
        0 => (0..shape.rows).collect(),
        n => vec![n as usize - 1],
    };
    let columns: Vec<usize> = match column {
        0 => (0..shape.cols).collect(),
        n => vec![n as usize - 1],
    };
    let mut cells = Vec::with_capacity(rows.len() * columns.len());
    for &r in &rows {
        for &c in &columns {
            cells.push(array.get(r, c).expect("an element of the array").clone());
        }
    }
    CalcValue::array(
        CalcArray::new(
            ArrayShape {
                rows: rows.len(),
                cols: columns.len(),
            },
            cells,
        )
        .expect("a selection of the array"),
    )
}

// ---------------------------------------------------------------------------------------------
// CHOOSE.

/// CHOOSE(index_num, value1, [value2], ...): index_num alone is asked for first and read as a
/// number, truncated; below 1 or beyond the values it is #VALUE!, and its error is the result.
/// Then only the chosen value is asked for, and it is the result as it is (a reference stays a
/// reference); an omitted value is Empty. An array index_num, for which Excel evaluates every
/// value, is not modelled.
fn choose(
    arguments: &mut LazyArguments<'_>,
    services: &mut EvalServices<'_>,
) -> Result<CalcValue, FunctionFault> {
    let site = Site::new(CHOOSE, services);
    let selector = arguments.argument(0)?;
    if matches!(selector.core_ref(), CoreRef::Array(_)) {
        return Err(FunctionFault::NotImplemented {
            id: CHOOSE,
            argument: Some(0),
        });
    }
    let n = match index_number(&site, 0, &selector) {
        Ok(n) => n.trunc(),
        Err(exit) => return Site::finish(Err(exit)),
    };
    if n < 1.0 || n > (arguments.count() - 1) as f64 {
        return Ok(CalcValue::error(WorksheetErrorCode::Value));
    }
    let chosen = arguments.argument(n as usize)?;
    Ok(if chosen.is_missing() {
        CalcValue::empty()
    } else {
        chosen
    })
}

// ---------------------------------------------------------------------------------------------
// ROW, COLUMN, ROWS, COLUMNS, AREAS.

/// ROW([reference]): the row number of a reference of one area and one row, and for an area of
/// several rows the column of every row's number (dc-b1l, measured in Excel 16.0.20430 on
/// 2026-10-25: the dynamic-array `=SUM(ROW(B3:B5))` is 12, `=ROWS(ROW(B3:B5))` 3). A legacy
/// formula takes the first of them where Excel writes `@` before the call (`=SUM(@ROW(B3:B5))`
/// is 3), which CalcFml's compile does. A reference of several areas is #REF!, and a value that is
/// not a reference (blank included) #VALUE!; an error argument is the result. An area of more than
/// [`POSITIONS_LIMIT`] rows is not modelled: its array is not built. Without an argument it is the
/// formula's own row, the caller the host names ([`crate::HostFacts::caller`]: a cell's formula,
/// or the cell a conditional format's formula is placed at, dc-6yw); not modelled when the host
/// names none.
fn row(
    arguments: &[CalcValue],
    services: &mut EvalServices<'_>,
) -> Result<CalcValue, FunctionFault> {
    let site = Site::new(ROW, services);
    if arguments.is_empty() {
        let caller = services.host().and_then(|host| host.caller());
        return Site::finish(match caller {
            Some(at) => site.number(f64::from(at.row())),
            None => Err(site.not_modelled(None)),
        });
    }
    Site::finish(position_of(&site, arguments, true))
}

/// COLUMN([reference]): ROW for columns; an area of several columns gives the row of every
/// column's number; without an argument, the caller's column.
fn column(
    arguments: &[CalcValue],
    services: &mut EvalServices<'_>,
) -> Result<CalcValue, FunctionFault> {
    let site = Site::new(COLUMN, services);
    if arguments.is_empty() {
        let caller = services.host().and_then(|host| host.caller());
        return Site::finish(match caller {
            Some(at) => site.number(f64::from(at.column())),
            None => Err(site.not_modelled(None)),
        });
    }
    Site::finish(position_of(&site, arguments, false))
}

/// The most rows or columns whose numbers ROW and COLUMN build as an array (every column of a
/// sheet; a whole column's 1,048,576 rows is refused rather than built).
const POSITIONS_LIMIT: u32 = 16_384;

fn position_of(site: &Site<'_>, arguments: &[CalcValue], of_row: bool) -> Step<CalcValue> {
    let Some(argument) = arguments.first() else {
        return Err(site.not_modelled(None));
    };
    let area = match argument.core_ref() {
        CoreRef::Error(code) => return Err(Exit::Error(code)),
        CoreRef::Reference(reference) => match reference.areas() {
            [area] => *area,
            _ => return Err(Exit::Error(WorksheetErrorCode::Ref)),
        },
        _ => return Err(Exit::Error(WorksheetErrorCode::Value)),
    };
    let (first, count) = if of_row {
        (area.first().row(), area.row_count())
    } else {
        (area.first().column(), area.column_count())
    };
    if count == 1 {
        return site.number(f64::from(first));
    }
    if count > POSITIONS_LIMIT {
        return Err(site.not_modelled(Some(0)));
    }
    let numbers = (first..first + count).map(|n| CalcValue::number(f64::from(n)));
    let rows: Vec<Vec<CalcValue>> = if of_row {
        numbers.map(|n| vec![n]).collect()
    } else {
        vec![numbers.collect()]
    };
    CalcArray::from_rows(rows)
        .map(CalcValue::array)
        .ok_or_else(|| site.not_modelled(Some(0)))
}

/// ROWS(array): the rows of a reference's single area or of an array; a single value has one; an
/// error is the result; a reference of several areas is #REF!, and a blank or omitted argument
/// #VALUE!.
fn rows(
    arguments: &[CalcValue],
    services: &mut EvalServices<'_>,
) -> Result<CalcValue, FunctionFault> {
    let site = Site::new(ROWS, services);
    Site::finish(extent(&site, &arguments[0], true))
}

/// COLUMNS(array): ROWS for columns.
fn columns(
    arguments: &[CalcValue],
    services: &mut EvalServices<'_>,
) -> Result<CalcValue, FunctionFault> {
    let site = Site::new(COLUMNS, services);
    Site::finish(extent(&site, &arguments[0], false))
}

fn extent(site: &Site<'_>, argument: &CalcValue, of_rows: bool) -> Step<CalcValue> {
    let count = match argument.core_ref() {
        CoreRef::Reference(reference) => match reference.areas() {
            [area] if of_rows => u64::from(area.row_count()),
            [area] => u64::from(area.column_count()),
            _ => return Err(Exit::Error(WorksheetErrorCode::Ref)),
        },
        CoreRef::Array(array) => {
            let shape = array.shape();
            (if of_rows { shape.rows } else { shape.cols }) as u64
        }
        CoreRef::Error(code) => return Err(Exit::Error(code)),
        CoreRef::Number(_) | CoreRef::Text(_) | CoreRef::Logical(_) => 1,
        CoreRef::Empty | CoreRef::Missing => {
            return Err(Exit::Error(WorksheetErrorCode::Value));
        }
    };
    site.number(count as f64)
}

/// AREAS(reference): the number of areas of a reference; an error is the result, and a value that
/// is not a reference (blank included) is #VALUE!.
fn areas(
    arguments: &[CalcValue],
    services: &mut EvalServices<'_>,
) -> Result<CalcValue, FunctionFault> {
    let site = Site::new(AREAS, services);
    Site::finish(match arguments[0].core_ref() {
        CoreRef::Reference(reference) => site.number(reference.area_count() as f64),
        CoreRef::Error(code) => Err(Exit::Error(code)),
        _ => Err(Exit::Error(WorksheetErrorCode::Value)),
    })
}
