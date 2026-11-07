//! Acceptance cases owned by the calc-func package, each registered by its exact name in
//! `crates/calc-func/acceptance.json`. Every case is `#[test]` natively and `#[wasm_bindgen_test]`
//! on wasm32, with no browser or Node.js configuration of its own: `cargo xtask accept` forces the
//! browser with `WASM_BINDGEN_USE_BROWSER`. The cases reach the value types only through
//! calc-func's re-export `calc_func::value`. The cfunc-m1.2 cases read the Excel capture of their
//! oracle workbook through calc-verdict, a dev-dependency.
//!
//! This test binary installs a counting global allocator: each thread counts its own allocation
//! calls, so a case measures only what its own thread allocates while libtest runs other cases on
//! other threads.

// Case names are fixed identities of the form `<bead>__<case>`.
#![allow(non_snake_case)]

use calc_func::value::{
    ArrayShape, CalcArray, CalcValue, CallableArityShape, CallableValue, CellKey, CoreRef,
    CoreValue, CubeValue, ExcelText, GridArea, GridError, IdAllocator, IdError, ImageFormat,
    ImageValue,
    MAX_COLUMNS, MAX_ROWS, NameId,
    NameKey, NameScope, NodeId, NumericMode, NumericTargetId, OpaqueCallable, ReferenceError,
    ReferenceLike, Selection, SheetId, TableId, WorkbookId, WorksheetErrorCode,
};
use calc_func::{
    ArgumentDemand, CellReader, EvalServices, FunctionCallScratch, FunctionCallTarget,
    FunctionFault, FunctionId, Notation, ParameterClass, PreparedArguments, RegistrationError,
    Unreadable, catalogue, invoke,
};
use calc_verdict::{
    Acquisition, CacheCellType, CalculationContext, CaptureFailure, CaptureRecord, CaptureSession,
    CellAddress, CellObservation, CellResult, Evaluation, EvaluationRecord, FormulaCell,
    FormulaKind, JudgeError, MissingCapture, Observation, Provenance, ResultSet, RunIdentity,
    STAGE_INVENTORY, STAGE_ISOLATION, SavedCache, SheetEntry, SheetVisibility, Verdict,
    VerdictPolicy, VerdictReport, judge, sha256_hex,
};
use std::alloc::{GlobalAlloc, Layout, System};
use std::any::Any;
use std::cell::Cell;
use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::hash::{BuildHasherDefault, DefaultHasher};
use std::hint::black_box;
use std::ops::ControlFlow;
use std::sync::Arc;

#[cfg(target_arch = "wasm32")]
use wasm_bindgen_test::wasm_bindgen_test;

/// Prints a line the acceptance runner keeps: on stdout natively, on the browser console (which
/// wasm-bindgen-test-runner forwards under `--nocapture`) on wasm32.
macro_rules! note {
    ($($arg:tt)*) => {{
        let line = format!($($arg)*);
        #[cfg(target_arch = "wasm32")]
        wasm_bindgen_test::console_log!("{}", line);
        #[cfg(not(target_arch = "wasm32"))]
        println!("{line}");
    }};
}

mod dc_xrxb_support;

// ---------------------------------------------------------------------------------------------
// The counting allocator.

thread_local! {
    /// Allocation calls (alloc, alloc_zeroed, realloc) made on this thread. Constant-initialized
    /// with no destructor, so reading it never allocates.
    static ALLOCATIONS: Cell<u64> = const { Cell::new(0) };
    /// The bytes those calls asked for (a reallocation's new size), counted the same way.
    static ALLOCATED_BYTES: Cell<u64> = const { Cell::new(0) };
}

struct CountingAllocator;

fn count_allocation(size: usize) {
    let _ = ALLOCATIONS.try_with(|count| count.set(count.get() + 1));
    let _ = ALLOCATED_BYTES.try_with(|bytes| bytes.set(bytes.get() + size as u64));
}

// SAFETY: every method forwards to `System` with the caller's arguments unchanged.
unsafe impl GlobalAlloc for CountingAllocator {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        count_allocation(layout.size());
        // SAFETY: forwarded unchanged; the caller upholds `GlobalAlloc::alloc`'s contract.
        unsafe { System.alloc(layout) }
    }

    unsafe fn alloc_zeroed(&self, layout: Layout) -> *mut u8 {
        count_allocation(layout.size());
        // SAFETY: forwarded unchanged; the caller upholds `GlobalAlloc::alloc_zeroed`'s contract.
        unsafe { System.alloc_zeroed(layout) }
    }

    unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, new_size: usize) -> *mut u8 {
        count_allocation(new_size);
        // SAFETY: forwarded unchanged; the caller upholds `GlobalAlloc::realloc`'s contract.
        unsafe { System.realloc(ptr, layout, new_size) }
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        // SAFETY: forwarded unchanged; the caller upholds `GlobalAlloc::dealloc`'s contract.
        unsafe { System.dealloc(ptr, layout) }
    }
}

#[global_allocator]
static GLOBAL: CountingAllocator = CountingAllocator;

/// Runs `body` and returns its result with the number of allocation calls this thread made
/// while it ran.
fn allocations_during<R>(body: impl FnOnce() -> R) -> (R, u64) {
    let before = ALLOCATIONS.with(Cell::get);
    let result = body();
    let after = ALLOCATIONS.with(Cell::get);
    (result, after - before)
}

/// Runs `body` and returns its result with the number of bytes this thread's allocation calls
/// asked for while it ran.
fn bytes_allocated_during<R>(body: impl FnOnce() -> R) -> (R, u64) {
    let before = ALLOCATED_BYTES.with(Cell::get);
    let result = body();
    let after = ALLOCATED_BYTES.with(Cell::get);
    (result, after - before)
}

// ---------------------------------------------------------------------------------------------
// cfunc_m0_1__grid_edges

/// The row and column numbers of A1-style text (column letters, then row digits), without any
/// bounds check. The test reads reference text itself: calc-value parses none.
fn decode_a1(text: &str) -> (u32, u32) {
    let digits_at = text
        .find(|c: char| c.is_ascii_digit())
        .unwrap_or_else(|| panic!("{text:?} has no row digits"));
    let (letters, digits) = text.split_at(digits_at);
    assert!(
        !letters.is_empty() && letters.bytes().all(|b| b.is_ascii_uppercase()),
        "{text:?} has no column letters"
    );
    let column = letters
        .bytes()
        .fold(0u32, |n, b| n * 26 + u32::from(b - b'A' + 1));
    let row = digits
        .parse::<u32>()
        .unwrap_or_else(|e| panic!("{text:?}: row digits: {e}"));
    (row, column)
}

/// The A1-style text of a row and column number.
fn encode_a1(row: u32, column: u32) -> String {
    let mut letters = Vec::new();
    let mut n = column;
    while n > 0 {
        letters.push(b'A' + ((n - 1) % 26) as u8);
        n = (n - 1) / 26;
    }
    letters.reverse();
    format!(
        "{}{row}",
        String::from_utf8(letters).expect("ASCII letters")
    )
}

#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test)]
fn cfunc_m0_1__grid_edges() {
    const CASE: &str = "cfunc_m0_1__grid_edges";
    let mut sheets: IdAllocator<SheetId> = IdAllocator::new();
    let first = sheets.issue().expect("the first sheet");
    let second = sheets.issue().expect("the second sheet");
    assert_ne!(first, second);

    // The grid's corners, decoded from A1 text by this test.
    let (a1_row, a1_column) = decode_a1("A1");
    let (xfd_row, xfd_column) = decode_a1("XFD1048576");
    assert_eq!((a1_row, a1_column), (1, 1));
    assert_eq!((xfd_row, xfd_column), (1_048_576, 16_384));
    assert_eq!((MAX_ROWS, MAX_COLUMNS), (xfd_row, xfd_column));

    // The edge keys and the full-grid area are built while this thread's allocations are counted.
    let ((a1, xfd, full, spanned, reversed), allocations) = allocations_during(|| {
        let a1 = CellKey::new(first, a1_row, a1_column);
        let xfd = CellKey::new(first, xfd_row, xfd_column);
        let full = GridArea::full_sheet(first);
        let spanned = match (a1, xfd) {
            (Ok(a1), Ok(xfd)) => Some((GridArea::new(a1, xfd), GridArea::new(xfd, a1))),
            _ => None,
        };
        black_box((a1, xfd, full, spanned.map(|s| s.0), spanned.map(|s| s.1)))
    });
    assert_eq!(
        allocations, 0,
        "building the edge keys and the full-grid area allocated {allocations} times"
    );
    // The same counter sees an allocation made on purpose, so the zero above is a measurement.
    let (_, control) = allocations_during(|| black_box(Box::new(0u64)));
    assert!(
        control >= 1,
        "the counting allocator saw no allocation for a Box"
    );
    note!(
        "ACCEPT-NOTE: {CASE}: allocation calls while building A1, XFD1048576 and the full-grid area: {allocations}; for one Box (control): {control}"
    );

    // A1 and XFD1048576 round-trip through the checked constructor and the accessors.
    let a1 = a1.expect("A1 is inside the grid");
    let xfd = xfd.expect("XFD1048576 is inside the grid");
    assert_eq!((a1.sheet(), a1.row(), a1.column()), (first, 1, 1));
    assert_eq!(
        (xfd.sheet(), xfd.row(), xfd.column()),
        (first, 1_048_576, 16_384)
    );
    assert_eq!(encode_a1(a1.row(), a1.column()), "A1");
    assert_eq!(encode_a1(xfd.row(), xfd.column()), "XFD1048576");

    // The full-grid area spans exactly those corners.
    assert_eq!((full.first(), full.last()), (a1, xfd));
    assert_eq!(spanned, Some(Ok(full)));
    assert_eq!(reversed, Some(Ok(full)));
    assert_eq!(
        (full.row_count(), full.column_count(), full.cell_count()),
        (1_048_576, 16_384, 17_179_869_184)
    );
    assert!(full.contains(a1) && full.contains(xfd));

    // Zero and out-of-bounds coordinates fail with the typed error.
    assert_eq!(CellKey::new(first, 0, 1), Err(GridError::RowOutOfRange(0)));
    assert_eq!(
        CellKey::new(first, 1, 0),
        Err(GridError::ColumnOutOfRange(0))
    );
    assert_eq!(
        CellKey::new(first, 1_048_577, 1),
        Err(GridError::RowOutOfRange(1_048_577))
    );
    assert_eq!(
        CellKey::new(first, 1, 16_385),
        Err(GridError::ColumnOutOfRange(16_385))
    );
    let (row, column) = decode_a1("XFE1");
    assert_eq!(
        CellKey::new(first, row, column),
        Err(GridError::ColumnOutOfRange(16_385))
    );
    let (row, column) = decode_a1("A1048577");
    assert_eq!(
        CellKey::new(first, row, column),
        Err(GridError::RowOutOfRange(1_048_577))
    );

    // The same row and column on two live sheets are two keys, and no area spans both sheets.
    let a1_second = CellKey::new(second, 1, 1).expect("A1 of the second sheet");
    assert_ne!(a1, a1_second);
    assert!(!full.contains(a1_second));
    assert_eq!(
        GridArea::new(a1, a1_second),
        Err(GridError::SheetMismatch {
            first,
            last: second
        })
    );

    // A released slot comes back under a new generation, and its keys stay distinct.
    sheets.release(first).expect("the first sheet is live");
    assert!(!sheets.is_live(first));
    assert_eq!(sheets.release(first), Err(IdError::NotLive));
    let reissued = sheets.issue().expect("a sheet in the released slot");
    assert_eq!(reissued.slot(), first.slot(), "the released slot is reused");
    assert_ne!(reissued.generation(), first.generation());
    assert_ne!(reissued, first);
    assert!(sheets.is_live(reissued) && sheets.is_live(second));
    let a1_reissued = CellKey::new(reissued, 1, 1).expect("A1 of the reissued sheet");
    assert_ne!(a1, a1_reissued);

    // A multi-area reference holds areas of one sheet: the same cell on two live sheets, or on a
    // released slot and its reissue, does not form one union, and no areas form no reference.
    assert_eq!(
        ReferenceLike::from_areas(vec![GridArea::cell(a1), GridArea::cell(a1_second)]),
        Err(ReferenceError::SheetMismatch {
            first,
            other: second
        })
    );
    assert_eq!(
        ReferenceLike::from_areas(vec![GridArea::cell(a1), GridArea::cell(a1_reissued)]),
        Err(ReferenceError::SheetMismatch {
            first,
            other: reissued
        })
    );
    assert_eq!(
        ReferenceLike::from_areas(Vec::new()),
        Err(ReferenceError::NoAreas)
    );
    let union = ReferenceLike::from_areas(vec![
        GridArea::cell(a1_second),
        GridArea::full_sheet(second),
    ])
    .expect("two areas of the second sheet");
    assert_eq!((union.sheet(), union.area_count()), (second, 2));

    note!(
        "ACCEPT-NOTE: {CASE}: sheet slot {} reissued at generation {} after generation {}",
        reissued.slot(),
        reissued.generation(),
        first.generation()
    );

    let keys = [
        (a1, "first"),
        (a1_second, "second"),
        (a1_reissued, "reissued"),
    ];
    let mut hashed: HashMap<CellKey, &str, BuildHasherDefault<DefaultHasher>> = HashMap::default();
    let mut ordered: BTreeMap<CellKey, &str> = BTreeMap::new();
    for (key, label) in keys {
        assert_eq!(
            hashed.insert(key, label),
            None,
            "{label} aliases another key"
        );
        assert_eq!(
            ordered.insert(key, label),
            None,
            "{label} aliases another key"
        );
    }
    for (key, label) in keys {
        assert_eq!(hashed[&key], label);
        assert_eq!(ordered[&key], label);
    }
    assert_eq!((hashed.len(), ordered.len()), (3, 3));
}

// ---------------------------------------------------------------------------------------------
// cfunc_m0_1__send_sync

const fn assert_send_sync<T: Send + Sync + ?Sized>() {}

/// Asserts at compile time that each listed type is `Send + Sync` and keeps the list for the case
/// to compare with calc-value's public exports.
macro_rules! send_sync_types {
    ($($ty:ty),* $(,)?) => {
        const _: () = { $(assert_send_sync::<$ty>();)* };
        const SEND_SYNC_TYPES: &[&str] = &[$(stringify!($ty)),*];
    };
}

send_sync_types![
    CalcValue,
    CubeValue,
    ImageFormat,
    ImageValue,
    CoreValue,
    CoreRef<'static>,
    CalcArray,
    ArrayShape,
    ExcelText,
    WorksheetErrorCode,
    ReferenceLike,
    ReferenceError,
    CellKey,
    GridArea,
    GridError,
    Selection,
    WorkbookId,
    SheetId,
    NameId,
    NameKey,
    NameScope,
    TableId,
    NodeId,
    IdAllocator<WorkbookId>,
    IdAllocator<SheetId>,
    IdAllocator<NameId>,
    IdAllocator<TableId>,
    IdAllocator<NodeId>,
    IdError,
    CallableArityShape,
    CallableValue,
    dyn OpaqueCallable,
    Arc<dyn OpaqueCallable>,
    NumericMode,
    NumericTargetId,
];

/// calc-value's source files by module name, read at compile time so the same checks run in the
/// browsers: the crate root and one file for each module it declares. The case checks that this
/// set equals the modules the root declares and that no other file declares a module.
const CALC_VALUE_SOURCES: [(&str, &str); 8] = [
    ("lib", include_str!("../../calc-value/src/lib.rs")),
    (
        "case_folding",
        include_str!("../../calc-value/src/case_folding.rs"),
    ),
    (
        "combining_marks",
        include_str!("../../calc-value/src/combining_marks.rs"),
    ),
    ("grid", include_str!("../../calc-value/src/grid.rs")),
    ("id", include_str!("../../calc-value/src/id.rs")),
    (
        "name_case",
        include_str!("../../calc-value/src/name_case.rs"),
    ),
    ("numeric", include_str!("../../calc-value/src/numeric.rs")),
    ("value", include_str!("../../calc-value/src/value.rs")),
];
/// calc-value's crate root. Its modules are private, so the root's `pub use` items and its own
/// `pub` declarations are the package's whole public surface; the case checks that no module is
/// public.
const CALC_VALUE_LIB: &str = CALC_VALUE_SOURCES[0].1;
/// calc-value's manifest, read at compile time so the same check runs in the browsers.
const CALC_VALUE_MANIFEST: &str = include_str!("../../calc-value/Cargo.toml");

/// The only manifest tables calc-value may have: no `[features]`, no `[lib]` or target table
/// that could move the crate root away from the files read above, no build dependencies and no
/// target-specific normal dependency.
const ALLOWED_MANIFEST_TABLES: [&str; 5] = [
    "[package]",
    "[package.metadata.dnacalc]",
    "[dependencies]",
    "[dev-dependencies]",
    "[target.'cfg(target_arch=\"wasm32\")'.dev-dependencies]",
];

/// The only conditional compilation calc-value may use, compared with whitespace removed: its
/// unit-test modules, and each unit test as `#[test]` natively and `#[wasm_bindgen_test]` on
/// wasm32.
const ALLOWED_CFG: [&str; 3] = [
    "cfg(test)",
    "cfg_attr(not(target_arch=\"wasm32\"),test)",
    "cfg_attr(target_arch=\"wasm32\",wasm_bindgen_test::wasm_bindgen_test)",
];

/// Identifiers that name a thread-bound shared pointer or cell (`std::rc`, `Rc<`, `RefCell`,
/// `Cell<` and their kin). The scan reads comments and strings too, so calc-value's sources do not
/// use these words anywhere.
const THREAD_BOUND: [&str; 7] = [
    "rc",
    "Rc",
    "RefCell",
    "Cell",
    "UnsafeCell",
    "OnceCell",
    "LazyCell",
];

/// The identifiers of a source text (maximal runs of ASCII letters, digits and `_` that do not
/// start with a digit), each with its byte offset. Comments and string literals are read like
/// code.
fn identifiers(text: &str) -> Vec<(usize, &str)> {
    let bytes = text.as_bytes();
    let word = |b: u8| b.is_ascii_alphanumeric() || b == b'_';
    let mut found = Vec::new();
    let mut at = 0;
    while at < bytes.len() {
        if word(bytes[at]) {
            let start = at;
            while at < bytes.len() && word(bytes[at]) {
                at += 1;
            }
            if !bytes[start].is_ascii_digit() {
                found.push((start, &text[start..at]));
            }
        } else {
            at += 1;
        }
    }
    found
}

/// The use of `cfg` or `cfg_attr` that starts at byte `start`, through its balanced parentheses
/// and with whitespace removed; up to the first other character when no parenthesis follows (as
/// in `cfg!`).
fn cfg_use(text: &str, start: usize) -> String {
    let mut depth = 0usize;
    let mut used = String::new();
    for c in text[start..].chars().filter(|c| !c.is_whitespace()) {
        used.push(c);
        match c {
            '(' => depth += 1,
            ')' if depth <= 1 => break,
            ')' => depth -= 1,
            _ if depth == 0 && !(c.is_ascii_alphanumeric() || c == '_') => break,
            _ => {}
        }
    }
    used
}

/// The modules a source text declares out of line (`mod name;`), whose code lives in files of
/// their own.
fn out_of_line_modules(text: &str) -> Vec<&str> {
    identifiers(text)
        .windows(2)
        .filter_map(|pair| {
            let ((keyword_at, keyword), (name_at, name)) = (pair[0], pair[1]);
            let between = &text[keyword_at + keyword.len()..name_at];
            let after = text[name_at + name.len()..].trim_start();
            (keyword == "mod" && between.trim().is_empty() && after.starts_with(';'))
                .then_some(name)
        })
        .collect()
}

/// Everything in one calc-value source file that could give a type a thread-bound or
/// configuration-dependent form: a thread-bound pointer or cell, a `cfg` other than
/// [`ALLOWED_CFG`], or a file the case did not read (an include macro or a `#[path]` module).
fn opt_outs(file: &str, text: &str) -> Vec<String> {
    let mut found = Vec::new();
    for (at, ident) in identifiers(text) {
        let line = text[..at].lines().count().max(1);
        if THREAD_BOUND.contains(&ident) {
            found.push(format!("{file}.rs:{line}: {ident}"));
        }
        if ident == "cfg" || ident == "cfg_attr" {
            let used = cfg_use(text, at);
            if !ALLOWED_CFG.contains(&used.as_str()) {
                found.push(format!("{file}.rs:{line}: {used}"));
            }
        }
    }
    let compact: String = text.chars().filter(|c| !c.is_whitespace()).collect();
    for marker in ["include!", "include_str!", "include_bytes!", "#[path"] {
        if compact.contains(marker) {
            found.push(format!("{file}.rs: {marker}"));
        }
    }
    found
}

/// The names a crate root makes public: those its `pub use` items export (under their `as`
/// names) and those of its own `pub struct`, `enum`, `trait`, `type` and `union` declarations.
fn exported_names(lib: &str) -> Vec<String> {
    let mut names = Vec::new();
    for item in lib.split("pub use ").skip(1) {
        let item = &item[..item.find(';').expect("a pub use item ends with ;")];
        let list = match (item.find('{'), item.rfind('}')) {
            (Some(open), Some(close)) => &item[open + 1..close],
            _ => &item[item.rfind("::").map_or(0, |at| at + 2)..],
        };
        names.extend(
            list.split(',')
                .map(|entry| entry.rsplit(" as ").next().unwrap_or_default().trim())
                .filter(|name| !name.is_empty())
                .map(str::to_owned),
        );
    }
    for keyword in ["struct", "enum", "trait", "type", "union"] {
        for declared in lib.split(&format!("pub {keyword} ")).skip(1) {
            let name: String = declared
                .chars()
                .take_while(|c| c.is_alphanumeric() || *c == '_')
                .collect();
            names.push(name);
        }
    }
    names
}

/// The non-empty lines of a Cargo manifest, each with its comment removed and trimmed.
fn manifest_lines(manifest: &str) -> impl Iterator<Item = String> + '_ {
    manifest
        .lines()
        .map(|line| line.split('#').next().unwrap_or_default().trim().to_owned())
        .filter(|line| !line.is_empty())
}

/// Hands back a copy of `value`; callable only for a `Send + Sync` type.
fn across<T: Send + Sync + Clone + 'static>(value: &T) -> T {
    value.clone()
}

#[derive(Debug, PartialEq)]
struct Token(u32);

impl OpaqueCallable for Token {
    fn as_any(&self) -> &dyn Any {
        self
    }
}

#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test)]
fn cfunc_m0_1__send_sync() {
    const CASE: &str = "cfunc_m0_1__send_sync";

    // Every type calc-value exports, other than its constants, is among the types asserted
    // Send + Sync at compile time above.
    assert!(
        !CALC_VALUE_LIB.contains("pub mod "),
        "calc-value has a public module, whose items this check does not read"
    );
    let exported = exported_names(CALC_VALUE_LIB);
    let types: Vec<&String> = exported
        .iter()
        .filter(|name| {
            !name
                .chars()
                .all(|c| c.is_ascii_uppercase() || c.is_ascii_digit() || c == '_')
        })
        .collect();
    assert!(
        types.len() >= 20 && exported.iter().any(|name| name == "CalcValue"),
        "the export list of calc-value was not read: {exported:?}"
    );
    for name in &types {
        let covered = SEND_SYNC_TYPES.iter().any(|asserted| {
            let asserted = asserted.replace(' ', "");
            asserted == **name
                || asserted == format!("dyn{name}")
                || asserted.starts_with(&format!("{name}<"))
        });
        assert!(
            covered,
            "calc-value exports {name}, which no Send + Sync assertion names"
        );
    }
    note!(
        "ACCEPT-NOTE: {CASE}: {} types asserted Send + Sync at compile time; they cover all {} type exports of calc-value",
        SEND_SYNC_TYPES.len(),
        types.len()
    );

    // No opt-out in the manifest: no [features] table (or any table outside the allowed set), no
    // optional dependency (whose name would be an implicit feature) and no build key.
    let lines: Vec<String> = manifest_lines(CALC_VALUE_MANIFEST)
        .map(|line| line.replace(' ', ""))
        .collect();
    assert!(
        lines.iter().any(|line| line == "[package]")
            && lines.iter().any(|line| line == "name=\"calc-value\""),
        "include_str! did not read calc-value's manifest"
    );
    // A key before the first table header is a root key, which could define a table (such as
    // `features.x = []`) without a header line.
    let root_keys = lines.iter().take_while(|line| !line.starts_with('['));
    let manifest_opt_outs: Vec<&String> = root_keys
        .chain(lines.iter().filter(|line| {
            (line.starts_with('[') && !ALLOWED_MANIFEST_TABLES.contains(&line.as_str()))
                || line.contains("optional=true")
                || line.starts_with("build=")
        }))
        .collect();
    assert!(
        manifest_opt_outs.is_empty(),
        "calc-value's manifest has an opt-out: {manifest_opt_outs:?}"
    );
    let tables: Vec<&String> = lines.iter().filter(|line| line.starts_with('[')).collect();
    note!(
        "ACCEPT-NOTE: {CASE}: calc-value's manifest tables are {tables:?}; no [features] table, no optional dependency and no build key"
    );

    // No opt-out in the code: the files read are exactly the crate root and the modules it
    // declares, and none of them holds a thread-bound pointer or cell, a cfg other than the unit
    // tests' or a file this case did not read.
    let declared: Vec<&str> = out_of_line_modules(CALC_VALUE_LIB);
    let mut read: Vec<&str> = CALC_VALUE_SOURCES[1..].iter().map(|(m, _)| *m).collect();
    let mut sorted_declared = declared.clone();
    sorted_declared.sort_unstable();
    read.sort_unstable();
    assert_eq!(
        sorted_declared, read,
        "calc-value's root declares modules {declared:?}, but the case read {read:?}"
    );
    let mut code_opt_outs = Vec::new();
    for (module, text) in CALC_VALUE_SOURCES {
        assert!(!text.is_empty(), "{module}.rs was not read");
        if module != "lib" {
            let nested = out_of_line_modules(text);
            assert!(
                nested.is_empty(),
                "{module}.rs declares modules in files this case does not read: {nested:?}"
            );
        }
        code_opt_outs.extend(opt_outs(module, text));
    }
    assert!(
        code_opt_outs.is_empty(),
        "calc-value's sources have an opt-out: {code_opt_outs:?}"
    );
    let cfg_uses = CALC_VALUE_SOURCES
        .iter()
        .flat_map(|(_, text)| identifiers(text))
        .filter(|(_, ident)| *ident == "cfg" || *ident == "cfg_attr")
        .count();
    let source_bytes: usize = CALC_VALUE_SOURCES.iter().map(|(_, text)| text.len()).sum();
    note!(
        "ACCEPT-NOTE: {CASE}: read {} calc-value source files ({source_bytes} bytes) for the modules {read:?} the root declares; no thread-bound pointer or cell, and all {cfg_uses} cfg uses are the unit tests' own",
        CALC_VALUE_SOURCES.len()
    );

    // Every kind of value, built through its public constructors, crosses a Send + Sync bound,
    // and a copy shares its Arc payload instead of duplicating it.
    let mut sheets: IdAllocator<SheetId> = IdAllocator::new();
    let sheet = across(&sheets.issue().expect("a sheet"));
    let a1 = CellKey::new(sheet, 1, 1).expect("A1");
    let c3 = CellKey::new(sheet, 3, 3).expect("C3");
    let area = across(&GridArea::new(a1, c3).expect("A1:C3"));

    let text = ExcelText::from_interop_assignment("payload");
    let text_copy = across(&text);
    assert_eq!(text_copy, text);
    assert!(
        std::ptr::eq(
            text_copy.utf16_code_units().as_ptr(),
            text.utf16_code_units().as_ptr()
        ),
        "a copy of text shares its code units"
    );

    let array = CalcArray::new(
        ArrayShape { rows: 2, cols: 3 },
        vec![
            CalcValue::number(1.5),
            CalcValue::text(text.clone()),
            CalcValue::logical(true),
            CalcValue::empty(),
            CalcValue::missing(),
            CalcValue::error(WorksheetErrorCode::NA),
        ],
    )
    .expect("a 2-by-3 array");
    let array_copy = across(&array);
    assert_eq!(array_copy, array);
    assert!(
        std::ptr::eq(array_copy.get(0, 0).unwrap(), array.get(0, 0).unwrap()),
        "a copy of an array shares its cells"
    );

    let reference = ReferenceLike::from_areas(vec![area, GridArea::cell(a1)]).expect("two areas");
    let reference_copy = across(&reference);
    assert_eq!(reference_copy.areas(), [area, GridArea::cell(a1)]);
    assert!(
        std::ptr::eq(reference_copy.areas().as_ptr(), reference.areas().as_ptr()),
        "a copy of a multi-area reference shares its areas"
    );

    let handle: Arc<dyn OpaqueCallable> = Arc::new(Token(7));
    let callable = CallableValue::new(CallableArityShape::exact(1), "LAMBDA(x, x)", handle.clone());
    let callable_value = CalcValue::callable(callable.clone());
    let callable_copy = across(&callable_value);
    let carried = callable_copy.callable_value().expect("a callable");
    assert!(
        std::ptr::eq(carried, callable_value.callable_value().unwrap()),
        "a copy of a callable value shares its callable"
    );
    assert!(Arc::ptr_eq(carried.handle(), &handle));
    assert_eq!(carried, &callable);
    assert_eq!(
        carried.handle().as_any().downcast_ref::<Token>(),
        Some(&Token(7))
    );
    assert_eq!(
        callable_copy.core(),
        &CoreValue::Error(WorksheetErrorCode::Calc)
    );

    // Natively, a value holding a callable moves into a spawned thread, which reads its token
    // through the handle, and comes back through join still sharing that handle.
    // wasm32-unknown-unknown has no threads; there the compile-time assertions carry the bound.
    #[cfg(not(target_arch = "wasm32"))]
    {
        let here = std::thread::current().id();
        let sent = callable_value.clone();
        let sent_handle = Arc::clone(&handle);
        let (returned, there, token_there, same_there) = std::thread::spawn(move || {
            let carried = sent
                .callable_value()
                .expect("a callable on the spawned thread");
            let token = carried
                .handle()
                .as_any()
                .downcast_ref::<Token>()
                .map(|token| token.0);
            let same = Arc::ptr_eq(carried.handle(), &sent_handle);
            (sent, std::thread::current().id(), token, same)
        })
        .join()
        .expect("the spawned thread hands the value back");
        assert_ne!(there, here, "the value was used on another thread");
        assert_eq!(token_there, Some(7));
        assert!(same_there, "the spawned thread saw the same handle");
        let back = returned
            .callable_value()
            .expect("a callable after the join");
        assert!(
            Arc::ptr_eq(back.handle(), &handle),
            "after the join the callable still shares its handle"
        );
        assert_eq!(returned, callable_value);
        note!(
            "ACCEPT-NOTE: {CASE}: a callable value moved into a spawned thread ({there:?}, not {here:?}), read token {token_there:?} there, and came back through join sharing its handle (Arc::ptr_eq)"
        );
    }
    #[cfg(target_arch = "wasm32")]
    {
        note!(
            "ACCEPT-NOTE: {CASE}: wasm32-unknown-unknown has no threads, so no thread is spawned; the Send + Sync bound is the compile-time assertion"
        );
    }

    let values = [
        CalcValue::number(1.5),
        CalcValue::text(text.clone()),
        CalcValue::logical(true),
        CalcValue::empty(),
        CalcValue::missing(),
        CalcValue::array(array.clone()),
        CalcValue::reference(reference.clone()),
        callable_value.clone(),
    ];
    let copies: Vec<CalcValue> = values.iter().map(across).collect();
    assert_eq!(copies.as_slice(), values.as_slice());
    assert_eq!(copies[0].as_number(), Some(1.5));
    assert_eq!(copies[1].as_text(), Some(&text));
    assert_eq!(copies[2].as_logical(), Some(true));
    assert!(copies[3].is_empty());
    assert!(copies[4].is_missing());
    assert_eq!(copies[5].as_array(), Some(&array));
    assert_eq!(copies[6].as_reference(), Some(&reference));
    assert_eq!(copies[7].as_error(), Some(WorksheetErrorCode::Calc));

    let codes = WorksheetErrorCode::all();
    for (index, code) in codes.iter().enumerate() {
        assert!(!codes[..index].contains(code), "{code:?} repeats");
        assert_eq!(across(&CalcValue::error(*code)).as_error(), Some(*code));
    }
    assert_eq!(codes.len(), 14);

    let fast = across(&NumericMode::default());
    assert_eq!((&fast, fast.target()), (&NumericMode::Fast, None));
    let target = NumericTargetId::new(
        "test-build",
        "test-channel",
        "test-compatibility",
        "test-environment",
    )
    .expect("a target with four parts");
    let exact = across(&NumericMode::Exact(target.clone()));
    let exact_target = exact.target().expect("an Exact target");
    assert_eq!(exact_target, &target);
    assert!(
        std::ptr::eq(
            exact_target.excel_build().as_ptr(),
            target.excel_build().as_ptr()
        ),
        "a copy of a numeric target shares its text"
    );

    let ids = (
        across(
            &IdAllocator::<WorkbookId>::new()
                .issue()
                .expect("a workbook"),
        ),
        across(&IdAllocator::<NameId>::new().issue().expect("a name")),
        across(&IdAllocator::<TableId>::new().issue().expect("a table")),
        across(&IdAllocator::<NodeId>::new().issue().expect("a node")),
    );
    assert_eq!(
        (ids.0.slot(), ids.1.slot(), ids.2.slot(), ids.3.slot()),
        (0, 0, 0, 0)
    );
    note!(
        "ACCEPT-NOTE: {CASE}: {} values and {} worksheet error codes were copied through a function that accepts only Send + Sync types (on this thread); text, array, multi-area reference, callable and numeric target copies share their Arc payloads",
        values.len(),
        codes.len()
    );
}

// ---------------------------------------------------------------------------------------------
// cfunc-m1.1: the generated catalogue and the call interface, reached only through calc-func's
// public API (catalogue(), invoke and FunctionCallTarget).

/// The surfaces the thin slice admits, written here independently of the catalogue: the
/// arithmetic, comparison and concatenation operators, SUM and IF. The catalogue holds each.
const ADMITTED: [(Notation, &str); 17] = [
    (Notation::Infix, "+"),
    (Notation::Infix, "-"),
    (Notation::Infix, "*"),
    (Notation::Infix, "/"),
    (Notation::Infix, "^"),
    (Notation::Prefix, "+"),
    (Notation::Prefix, "-"),
    (Notation::Postfix, "%"),
    (Notation::Infix, "="),
    (Notation::Infix, "<>"),
    (Notation::Infix, "<"),
    (Notation::Infix, "<="),
    (Notation::Infix, ">"),
    (Notation::Infix, ">="),
    (Notation::Infix, "&"),
    (Notation::Call, "SUM"),
    (Notation::Call, "IF"),
];

/// Call-site arguments that record each argument the call interface asks for. `asked` is
/// reserved up front, so recording allocates nothing.
struct Recorded {
    values: Vec<CalcValue>,
    asked: Vec<usize>,
}

impl Recorded {
    fn new(values: Vec<CalcValue>) -> Self {
        Recorded {
            values,
            asked: Vec::with_capacity(16),
        }
    }

    /// The numbers 1 to `count`.
    fn numbers(count: usize) -> Self {
        Self::new((1..=count).map(|n| CalcValue::number(n as f64)).collect())
    }
}

impl PreparedArguments for Recorded {
    fn count(&self) -> usize {
        self.values.len()
    }

    fn argument(&mut self, index: usize) -> CalcValue {
        self.asked.push(index);
        self.values[index].clone()
    }
}

/// One call through `calc_func::invoke` with fresh services (no cell reader) and scratch.
fn run(
    target: FunctionCallTarget<'_>,
    arguments: &mut Recorded,
    mode: &NumericMode,
) -> Result<CalcValue, FunctionFault> {
    invoke(
        target,
        arguments,
        &mut EvalServices::new(mode),
        &mut FunctionCallScratch::new(),
    )
}

fn surface(notation: Notation, spelling: &str) -> FunctionCallTarget<'static> {
    catalogue()
        .resolve_surface(notation, spelling)
        .unwrap_or_else(|e| panic!("{notation:?} {spelling:?} does not resolve: {e:?}"))
}

fn a1_reference() -> CalcValue {
    let mut sheets: IdAllocator<SheetId> = IdAllocator::new();
    let sheet = sheets.issue().expect("a sheet");
    CalcValue::reference(ReferenceLike::cell(CellKey::new(sheet, 1, 1).expect("A1")))
}

#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test)]
fn cfunc_m1_1__catalogue_consistency() {
    const CASE: &str = "cfunc_m1_1__catalogue_consistency";
    let catalogue = catalogue();
    let fast = NumericMode::Fast;

    // Dispatch: the ID-ordered table holds each ID once.
    let dispatch: Vec<FunctionId> = catalogue.targets().map(FunctionCallTarget::id).collect();
    assert!(
        dispatch.windows(2).all(|pair| pair[0] < pair[1]),
        "the dispatch table is not in strictly increasing ID order: {dispatch:?}"
    );
    let dispatch_ids: BTreeSet<FunctionId> = dispatch.iter().copied().collect();

    // Names: the surface table holds each surface once and every admitted surface, and each
    // surface and each ID resolve to the same entry; a named call also in lower case.
    let mut surfaces = BTreeSet::new();
    let mut named_ids = BTreeSet::new();
    let mut help_ids = BTreeSet::new();
    let mut oracle_ids = BTreeSet::new();
    for target in catalogue.surfaces() {
        assert!(
            surfaces.insert((target.notation(), target.spelling().to_owned())),
            "{target:?} appears twice in the surface table"
        );
        named_ids.insert(target.id());
        assert_eq!(
            catalogue.resolve_surface(target.notation(), target.spelling()),
            Ok(target)
        );
        assert_eq!(
            catalogue.resolve_surface(target.notation(), &target.spelling().to_ascii_lowercase()),
            Ok(target)
        );
        let by_id = catalogue
            .resolve(target.id())
            .expect("a named entry resolves by ID");
        assert_eq!(
            (by_id.notation(), by_id.spelling()),
            (target.notation(), target.spelling())
        );

        // Help: the parameters cover the arity; the last one repeats exactly when the arity
        // admits more arguments than there are names.
        let arity = target.arity();
        let help = target.signature_help();
        let required = help.parameters.iter().filter(|p| !p.optional).count();
        let repeating: Vec<usize> = (0..help.parameters.len())
            .filter(|&i| help.parameters[i].repeats)
            .collect();
        assert_eq!(required, arity.min, "{target:?}: required parameters");
        assert!(
            help.parameters.len() <= arity.max,
            "{target:?}: too many names"
        );
        if arity.max > help.parameters.len() {
            assert_eq!(repeating, [help.parameters.len() - 1], "{target:?}");
        } else {
            assert!(repeating.is_empty(), "{target:?}");
        }
        assert!(
            help.display.contains(target.spelling()),
            "{target:?}: {}",
            help.display
        );
        help_ids.insert(target.id());

        // Oracle selection: the arity's minimum number of operands gives a formula, one more
        // than the maximum gives none.
        let operands: Vec<String> = (1..=arity.min).map(|n| n.to_string()).collect();
        let operands: Vec<&str> = operands.iter().map(String::as_str).collect();
        let formula = target.oracle_formula(&operands).expect("an oracle formula");
        assert!(formula.starts_with('=') && formula.contains(target.spelling()));
        let too_many: Vec<&str> = vec!["1"; arity.max + 1];
        assert_eq!(target.oracle_formula(&too_many), None, "{target:?}");
        oracle_ids.insert(target.id());
    }
    let expected: BTreeSet<(Notation, String)> = ADMITTED
        .iter()
        .map(|(notation, spelling)| (*notation, (*spelling).to_owned()))
        .collect();
    assert!(
        expected.is_subset(&surfaces),
        "admitted surfaces missing from the catalogue: {:?}",
        expected.difference(&surfaces).collect::<Vec<_>>()
    );

    // Arity and dispatch: every ID of the dispatch table, invoked with its arity's minimum,
    // reaches its own entry, which computes a value or refuses under its own ID; outside its
    // arity no argument is asked for. An eager entry is asked for each argument once, in order,
    // and admission stops at the first argument it refuses (a number at a reference parameter);
    // a lazy entry is asked for its discriminator first and then at most one held-back argument.
    // Every entry admits an argument except TRUE, FALSE and NA, which take none (listed below), and
    // every entry that admits one requires one except ROW and COLUMN, whose reference is optional.
    let mut arity_ids = BTreeSet::new();
    let mut computed = Vec::new();
    let mut refused = Vec::new();
    let mut lazy = Vec::new();
    let mut no_argument = Vec::new();
    for target in catalogue.targets() {
        let arity = target.arity();
        match target.notation() {
            Notation::Infix => assert_eq!((arity.min, arity.max), (2, 2), "{target:?}"),
            Notation::Prefix | Notation::Postfix => {
                assert_eq!((arity.min, arity.max), (1, 1), "{target:?}")
            }
            Notation::Call => assert!(arity.min <= arity.max),
        }
        if arity.max == 0 {
            no_argument.push(target.id());
        } else {
            // ROW and COLUMN take an optional reference (their pages: ROW([reference]),
            // COLUMN([reference])), RANDARRAY optional dimensions (F1, dc-8dq), and SHEET and
            // SHEETS an optional value or reference (F2, dc-ip6); every other entry that admits
            // an argument requires one.
            assert!(
                arity.min >= 1
                    || ["ROW", "COLUMN", "RANDARRAY", "SHEET", "SHEETS"]
                        .contains(&target.spelling()),
                "{target:?}: every entry that admits an argument but ROW, COLUMN, RANDARRAY, SHEET and SHEETS requires one"
            );
            assert!(
                target.parameter_class(arity.max - 1).is_some(),
                "{target:?}"
            );
        }
        assert_eq!(target.parameter_class(arity.max), None, "{target:?}");

        let demand = target.argument_demand();
        let mut at_minimum = Recorded::numbers(arity.min);
        // The arguments an eager call asks for: all of them, in order, unless one is refused at
        // admission, which ends the call after asking for it.
        let mut asked_through = arity.min;
        match run(target, &mut at_minimum, &fast) {
            Ok(value) => computed.push((target.id(), value)),
            Err(FunctionFault::NotImplemented { id, .. }) => {
                assert_eq!(id, target.id(), "{target:?} refused under another ID");
                refused.push(target.id());
            }
            // A reference-only parameter refuses a number at admission, before the kernel runs.
            Err(FunctionFault::ReferenceRequired { id, index }) => {
                assert_eq!(id, target.id(), "{target:?} refused under another ID");
                assert_eq!(
                    target.parameter_class(index),
                    Some(ParameterClass::Reference),
                    "{target:?} refused a number at a parameter that admits one"
                );
                asked_through = index + 1;
                refused.push(target.id());
            }
            Err(fault) => panic!("{target:?} at its minimum arity: {fault:?}"),
        }
        if demand.is_lazy() {
            lazy.push(target.id());
            assert_eq!(
                at_minimum.asked.first().copied(),
                demand.discriminator(),
                "{target:?} asked {:?}",
                at_minimum.asked
            );
            assert!(
                at_minimum.asked.len() <= 2
                    && at_minimum.asked[1..]
                        .iter()
                        .all(|&index| demand.holds_back(index)),
                "{target:?} asked {:?}",
                at_minimum.asked
            );
        } else {
            let in_order: Vec<usize> = (0..asked_through).collect();
            assert_eq!(at_minimum.asked, in_order, "{target:?}");
        }

        let mut too_many = Recorded::numbers(arity.max + 1);
        assert_eq!(
            run(target, &mut too_many, &fast),
            Err(FunctionFault::ArityMismatch {
                id: target.id(),
                supplied: arity.max + 1
            })
        );
        assert!(too_many.asked.is_empty());
        // An entry whose arguments are all optional has no call with too few.
        if let Some(fewer) = arity.min.checked_sub(1) {
            let mut too_few = Recorded::numbers(fewer);
            assert_eq!(
                run(target, &mut too_few, &fast),
                Err(FunctionFault::ArityMismatch {
                    id: target.id(),
                    supplied: fewer
                })
            );
            assert!(too_few.asked.is_empty());
        }
        arity_ids.insert(target.id());
    }
    assert_eq!(named_ids, dispatch_ids, "name table against dispatch table");
    assert_eq!(help_ids, dispatch_ids, "help against dispatch");
    assert_eq!(arity_ids, dispatch_ids, "arity against dispatch");
    assert_eq!(
        oracle_ids, dispatch_ids,
        "oracle selection against dispatch"
    );

    let sum = surface(Notation::Call, "SUM");
    let iff = surface(Notation::Call, "IF");
    assert!(
        computed.contains(&(sum.id(), CalcValue::number(1.0))),
        "SUM(1) computes 1: {computed:?}"
    );
    let fallbacks = [
        surface(Notation::Call, "IFERROR").id(),
        surface(Notation::Call, "IFNA").id(),
    ];
    // CHOOSE evaluates every value only when index_num is an array (its page), so it holds its
    // values back as IF holds its branches.
    let choose = surface(Notation::Call, "CHOOSE");
    assert_eq!(
        lazy,
        [iff.id(), fallbacks[0], fallbacks[1], choose.id()],
        "only IF, IFERROR, IFNA and CHOOSE are lazy"
    );
    // F1 (dc-8dq) adds PI and RAND, dc-nzpw NA, and F2 (dc-ip6) NOW and TODAY, to the entries
    // that take no argument.
    let constants: BTreeSet<FunctionId> = ["TRUE", "FALSE", "NA", "PI", "RAND", "NOW", "TODAY"]
        .into_iter()
        .filter_map(|name| catalogue.resolve_surface(Notation::Call, name).ok())
        .map(|target| target.id())
        .collect();
    assert!(
        no_argument.iter().all(|id| constants.contains(id))
            && constants.iter().all(|id| no_argument.contains(id)),
        "only TRUE, FALSE, NA, PI, RAND, NOW and TODAY take no argument: {no_argument:?}"
    );
    note!(
        "ACCEPT-NOTE: {CASE}: {} catalogue IDs; the dispatch, name, help, arity and oracle views each enumerate the same {}; at the minimum arity {} entries compute ({:?}) and {} refuse under their own ID ({:?}); lazy entries {:?}; entries that take no argument {:?}",
        dispatch_ids.len(),
        dispatch_ids.len(),
        computed.len(),
        computed.iter().map(|(id, _)| *id).collect::<Vec<_>>(),
        refused.len(),
        refused,
        lazy,
        no_argument
    );

    // SUM(1, 2, 3) and SUM(0.5, -2, 10) through calc_func::invoke, by FunctionCallTarget::invoke.
    let mut one_two_three = Recorded::numbers(3);
    let six = sum.invoke(
        &mut one_two_three,
        &mut EvalServices::new(&fast),
        &mut FunctionCallScratch::new(),
    );
    assert_eq!(
        six.as_ref()
            .map(|value| value.as_number().map(f64::to_bits)),
        Ok(Some(0x4018_0000_0000_0000))
    );
    assert_eq!(one_two_three.asked, [0, 1, 2]);
    let mut not_positions = Recorded::new(vec![
        CalcValue::number(0.5),
        CalcValue::number(-2.0),
        CalcValue::number(10.0),
    ]);
    let eight_and_a_half = sum.invoke(
        &mut not_positions,
        &mut EvalServices::new(&fast),
        &mut FunctionCallScratch::new(),
    );
    assert_eq!(
        eight_and_a_half
            .as_ref()
            .map(|value| value.as_number().map(f64::to_bits)),
        Ok(Some(0x4021_0000_0000_0000))
    );
    assert_eq!(
        (
            sum.arity().min,
            sum.arity().max,
            iff.arity().min,
            iff.arity().max
        ),
        (1, 255, 2, 3)
    );

    // The scratch is the caller's: a second call on the same scratch allocates nothing.
    let mut scratch = FunctionCallScratch::new();
    let mut first = Recorded::numbers(3);
    let mut second = Recorded::numbers(3);
    let (first_result, first_allocations) = allocations_during(|| {
        black_box(invoke(
            sum,
            &mut first,
            &mut EvalServices::new(&fast),
            &mut scratch,
        ))
    });
    let (second_result, second_allocations) = allocations_during(|| {
        black_box(invoke(
            sum,
            &mut second,
            &mut EvalServices::new(&fast),
            &mut scratch,
        ))
    });
    assert_eq!(first_result, Ok(CalcValue::number(6.0)));
    assert_eq!(second_result, Ok(CalcValue::number(6.0)));
    assert!(
        first_allocations >= 1,
        "the first call grows the fresh scratch, and the counter saw no allocation"
    );
    assert_eq!(
        second_allocations, 0,
        "a call on warm scratch allocated {second_allocations} times"
    );
    note!(
        "ACCEPT-NOTE: {CASE}: SUM(1,2,3) through calc_func::invoke = {:?} (bits {:#018x}); SUM(0.5,-2,10) = {:?} (bits {:#018x}); arguments asked {:?}; allocation calls: {first_allocations} on fresh scratch, {second_allocations} on warm scratch",
        six.as_ref().ok().and_then(CalcValue::as_number),
        0x4018_0000_0000_0000u64,
        eight_and_a_half
            .as_ref()
            .ok()
            .and_then(CalcValue::as_number),
        0x4021_0000_0000_0000u64,
        one_two_three.asked
    );

    // Under Exact, SUM refuses at its first arithmetic site instead of adding under Fast.
    let target_id = NumericTargetId::new(
        "test-build",
        "test-channel",
        "test-compatibility",
        "test-environment",
    )
    .expect("a target");
    let mut under_exact = Recorded::numbers(3);
    assert_eq!(
        run(sum, &mut under_exact, &NumericMode::Exact(target_id)),
        Err(FunctionFault::ExactUnavailable { id: sum.id() })
    );

    // IF's demand holds its branches back: the call interface asks for the condition alone
    // before it decides, then for the chosen branch only. The eager "=" is asked for both of the
    // same kind of arguments.
    let demand = iff.argument_demand();
    assert_eq!(demand, ArgumentDemand::BranchOnCondition);
    assert_eq!(demand.discriminator(), Some(0));
    assert_eq!(
        (
            demand.holds_back(0),
            demand.holds_back(1),
            demand.holds_back(2)
        ),
        (false, true, true)
    );
    let mut branches = Recorded::new(vec![
        CalcValue::logical(true),
        CalcValue::error(WorksheetErrorCode::Div0),
        CalcValue::error(WorksheetErrorCode::NA),
    ]);
    assert_eq!(
        run(iff, &mut branches, &fast),
        Ok(CalcValue::error(WorksheetErrorCode::Div0))
    );
    assert_eq!(branches.asked, [0, 1], "the unchosen branch is never asked");
    let mut undecided = Recorded::new(vec![
        CalcValue::error(WorksheetErrorCode::Ref),
        CalcValue::error(WorksheetErrorCode::Div0),
        CalcValue::error(WorksheetErrorCode::NA),
    ]);
    assert_eq!(
        run(iff, &mut undecided, &fast),
        Ok(CalcValue::error(WorksheetErrorCode::Ref))
    );
    assert_eq!(
        undecided.asked,
        [0],
        "no branch is asked before the condition"
    );
    let equal = surface(Notation::Infix, "=");
    assert_eq!(equal.argument_demand(), ArgumentDemand::Eager);
    let mut operands = Recorded::new(vec![
        CalcValue::logical(true),
        CalcValue::error(WorksheetErrorCode::Div0),
    ]);
    assert_eq!(
        run(equal, &mut operands, &fast),
        Ok(CalcValue::error(WorksheetErrorCode::Div0))
    );
    assert_eq!(operands.asked, [0, 1]);
    note!(
        "ACCEPT-NOTE: {CASE}: IF demand {demand:?}, discriminator {:?}; invoke asked for {:?} of IF(TRUE,#DIV/0!,#N/A), {:?} of IF(#REF!,#DIV/0!,#N/A) and {:?} of the 2 arguments of the eager = operator",
        demand.discriminator(),
        branches.asked,
        undecided.asked,
        operands.asked
    );

    // Parameter classes: an operator's value parameter and IF's condition refuse a reference;
    // SUM's parameters admit one, and without a cell reader SUM refuses to read it. A direct
    // array passes admission: an operator refuses to lift over it, and SUM adds its numbers.
    let plus = surface(Notation::Infix, "+");
    assert_eq!(plus.parameter_class(0), Some(ParameterClass::Value));
    assert_eq!(
        (
            iff.parameter_class(0),
            iff.parameter_class(1),
            iff.parameter_class(2)
        ),
        (
            Some(ParameterClass::Value),
            Some(ParameterClass::ValueOrReference),
            Some(ParameterClass::ValueOrReference)
        )
    );
    assert_eq!(
        sum.parameter_class(254),
        Some(ParameterClass::ValueOrReference)
    );
    let mut plus_reference = Recorded::new(vec![CalcValue::number(1.0), a1_reference()]);
    assert_eq!(
        run(plus, &mut plus_reference, &fast),
        Err(FunctionFault::ReferenceArgument {
            id: plus.id(),
            index: 1
        })
    );
    let mut if_reference = Recorded::new(vec![a1_reference(), CalcValue::number(1.0)]);
    assert_eq!(
        run(iff, &mut if_reference, &fast),
        Err(FunctionFault::ReferenceArgument {
            id: iff.id(),
            index: 0
        })
    );
    assert_eq!(if_reference.asked, [0]);
    let mut sum_reference = Recorded::new(vec![CalcValue::number(1.0), a1_reference()]);
    assert_eq!(
        run(sum, &mut sum_reference, &fast),
        Err(FunctionFault::ReferenceUnreadable {
            id: sum.id(),
            index: 1
        })
    );
    let array = CalcValue::array(CalcArray::from_scalar(CalcValue::number(2.0)));
    let mut plus_array = Recorded::new(vec![CalcValue::number(1.0), array.clone()]);
    assert_eq!(
        run(plus, &mut plus_array, &fast),
        Err(FunctionFault::NotImplemented {
            id: plus.id(),
            argument: Some(1)
        })
    );
    let mut sum_array = Recorded::new(vec![array, CalcValue::number(1.0)]);
    assert_eq!(run(sum, &mut sum_array, &fast), Ok(CalcValue::number(3.0)));

    // Help text and oracle formulas for one entry of each notation.
    let helps = [
        (sum, "SUM(number1, [number2], ...)"),
        (iff, "IF(logical_test, value_if_true, [value_if_false])"),
        (plus, "number1 + number2"),
        (surface(Notation::Prefix, "-"), "-number"),
        (surface(Notation::Postfix, "%"), "number%"),
        (surface(Notation::Infix, "&"), "text1 & text2"),
    ];
    for (target, display) in helps {
        assert_eq!(target.signature_help().display, display);
    }
    let formulas = [
        (sum, &["1", "2", "3"][..], "=SUM(1,2,3)"),
        (iff, &["TRUE", "1", "2"][..], "=IF(TRUE,1,2)"),
        (plus, &["1", "2"][..], "=1+2"),
        (surface(Notation::Prefix, "-"), &["1"][..], "=-1"),
        (surface(Notation::Postfix, "%"), &["50"][..], "=50%"),
        (surface(Notation::Infix, "<>"), &["1", "2"][..], "=1<>2"),
    ];
    for (target, operands, formula) in formulas {
        assert_eq!(target.oracle_formula(operands).as_deref(), Some(formula));
    }
    assert_eq!(sum.oracle_formula(&[]), None);
    note!(
        "ACCEPT-NOTE: {CASE}: help {:?}; oracle formula {:?}",
        sum.signature_help().display,
        sum.oracle_formula(&["1", "2", "3"])
    );
}

#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test)]
fn cfunc_m1_1__unknown_function() {
    const CASE: &str = "cfunc_m1_1__unknown_function";
    let catalogue = catalogue();
    let admitted: BTreeSet<u32> = catalogue.targets().map(|t| t.id().raw()).collect();
    for (notation, spelling) in ADMITTED {
        assert!(
            admitted.contains(&surface(notation, spelling).id().raw()),
            "{notation:?} {spelling:?} resolves outside the dispatch table"
        );
    }

    // Unknown IDs: the neighbours of every admitted ID, the IDs with the top bit of each member
    // number flipped, and both extremes, wherever the catalogue does not admit them.
    let mut unknown = BTreeSet::new();
    for raw in &admitted {
        for near in [raw.wrapping_sub(1), raw.wrapping_add(1), raw ^ 0x8000] {
            unknown.insert(near);
        }
    }
    unknown.extend([0, u32::MAX]);
    unknown.retain(|raw| !admitted.contains(raw));
    for raw in &unknown {
        let id = FunctionId::from_raw(*raw);
        assert_eq!(catalogue.resolve(id), Err(RegistrationError::UnknownId(id)));
    }
    for target in catalogue.targets() {
        assert_eq!(
            catalogue.resolve(FunctionId::from_raw(target.id().raw())),
            Ok(target),
            "an admitted ID round-trips through its raw value"
        );
    }

    // Unknown surfaces, including a known spelling under another notation.
    let unknown_surfaces = [
        (Notation::Call, "NOSUCHFUNCTION"),
        (Notation::Call, "SU"),
        (Notation::Call, "SUMS"),
        (Notation::Call, " SUM"),
        (Notation::Call, "S.UM"),
        (Notation::Call, ""),
        (Notation::Call, "\u{FF33}\u{FF35}\u{FF2D}"),
        (Notation::Call, "+"),
        (Notation::Infix, "SUM"),
        (Notation::Infix, "%"),
        (Notation::Infix, "=="),
        (Notation::Infix, "=<"),
        (Notation::Prefix, "*"),
        (Notation::Prefix, "&"),
        (Notation::Postfix, "+"),
    ];
    for (notation, spelling) in unknown_surfaces {
        assert_eq!(
            catalogue.resolve_surface(notation, spelling),
            Err(RegistrationError::UnknownSurface {
                notation,
                spelling: spelling.to_owned()
            })
        );
    }
    let sum = surface(Notation::Call, "SUM");
    for spelling in ["sum", "Sum", "sUM"] {
        assert_eq!(
            catalogue
                .resolve_surface(Notation::Call, spelling)
                .map(FunctionCallTarget::id),
            Ok(sum.id())
        );
    }
    note!(
        "ACCEPT-NOTE: {CASE}: {} unknown IDs refused with UnknownId; {} unknown surfaces refused with UnknownSurface; {} admitted IDs round-trip through their raw values",
        unknown.len(),
        unknown_surfaces.len(),
        admitted.len()
    );
}

// ---------------------------------------------------------------------------------------------
// cfunc-m1.2: worksheet coercion, the arithmetic, comparison and concatenation operators, SUM
// over direct and referenced arguments, and IF, all through calc_func::invoke on catalogue
// targets.
//
// Each oracle row runs one call and asserts a result written here by hand. That assertion is the
// code's own consistency check and says nothing about Excel; the Excel comparison against the
// committed capture, below, is the evidence. A row labelled `unmeasured choice` encodes a choice
// of this implementation that its hand-written expectation alone does not settle (which
// operand's error wins, the order SUM reads a range in, an overflow against a later error, a
// zero's sign); the comparison's verdict on its cell does.
//
// Each row names the bead that owns the Excel behaviour it shows. A row of cfunc-m1.2 computes a
// value, and the Excel comparison must find it equal to Excel's. A row whose behaviour another
// bead owns (the near-zero adjustment of a formula's final + or -, ^ beyond the powers computed
// here, number text read by locale rules, an array condition in a legacy formula, subnormal
// results, number text read by COUNT and by COUNTIF's criteria) is run and asserted the same
// way, whether calc-func computes it or refuses it, and is published to the comparison as not
// modeled, with its owner: it is counted as not modeled and never as matched.
//
// The Excel comparison goes through the one capture route and the one verdict path.
// `tests/fixtures/cfunc_m1_2/oracle.xlsx`, written by the committed generator
// `tests/fixtures/make-fixtures.ps1`, holds every row's formula in column D of a sheet named after
// its case, in the order the case runs its rows, beside the cells the rows read; each case checks
// that its sheet holds exactly those formulas and cells. The owner's `cargo xtask capture` run
// writes Excel's raw values for that workbook, as a CaptureRecord version 2, to
// `tests/captures/cfunc_m1_2/oracle.capture.json`. Each case then runs the rows of both sheets,
// publishes each row's calc_func::invoke result as a calc_verdict ResultSet and each recorded
// invoke call as an EvaluationRecord, and asserts on the VerdictReport that calc_verdict::judge
// computes from them and the capture. Before a capture is taken that path holds the
// absent-capture marker, and each case prints ACCEPT-NOT-RUN with calc-verdict's MissingCapture,
// so the runner records it as not run.
//
// The retained witness `migration/witnesses/w111-extended-power.json` holds Excel's results of the
// POWER function. The mixed-operand case applies it to the ^ operator, which assumes that ^
// computes POWER. Of the oracle workbook's ^ rows that need a power algorithm (=2^3, =10^-2 and
// =2^0.5), which ^ computes by OxFunc's power kernel since F1 (dc-8dq), the witness holds the
// arguments of =10^-2 alone; the case compares Excel's captured value of each such row with the
// witness's row for its arguments.

/// The oracle workbook, as committed (its bytes are kept across checkouts by `.gitattributes`).
const ORACLE_WORKBOOK: &[u8] = include_bytes!("fixtures/cfunc_m1_2/oracle.xlsx");
const ORACLE_WORKBOOK_PATH: &str = "crates/calc-func/tests/fixtures/cfunc_m1_2/oracle.xlsx";
/// Excel's capture of the oracle workbook, or the absent-capture marker until the owner takes it.
const ORACLE_CAPTURE: &[u8] = include_bytes!("captures/cfunc_m1_2/oracle.capture.json");
/// The capture worker, whose content hash the capture's tool revision must name.
const CAPTURE_WORKER: &[u8] =
    include_bytes!("../../../platform/calc-oracle-excel/worker/capture-worker.ps1");
/// The case the capture is recorded under; both cases compare their rows with it.
const CAPTURE_CASE: &str = "cfunc_m1_2__mixed_operands";
/// The owner's command that writes the capture.
const CAPTURE_COMMAND: &str = "cargo xtask capture --fixture crates/calc-func/tests/fixtures/cfunc_m1_2/oracle.xlsx --case cfunc_m1_2__mixed_operands --out crates/calc-func/tests/captures/cfunc_m1_2/oracle.capture.json";
/// The column (D) of the oracle sheets that holds the formulas, one row per oracle row from row 1.
const FORMULA_COLUMN: u32 = 4;
/// Every workbook the committed generator `tests/fixtures/make-fixtures.ps1` writes, by its path
/// under `tests/fixtures`, in path order.
#[cfg(all(windows, not(target_arch = "wasm32")))]
const GENERATED_WORKBOOKS: [&str; 11] = [
    "cdoc_jpha/oracle.xlsx",
    "cfunc_m1_2/oracle.xlsx",
    "cfunc_m3_2/oracle.xlsx",
    "cfunc_m3_4/oracle.xlsx",
    "cfunc_m3_5/oracle.xlsx",
    "cfunc_m3_6/oracle.xlsx",
    "dc_e00t/oracle.xlsx",
    "dc_fwpz/oracle.xlsx",
    "dc_nzpw/oracle.xlsx",
    "dc_q9rb/oracle.xlsx",
    "dc_xrxb/oracle.xlsx",
];

/// A worksheet the test holds and calc-func reads through `CellReader`. Every area asked for and
/// the number of cells visited are recorded.
struct TestSheet {
    id: SheetId,
    cells: BTreeMap<(u32, u32), CalcValue>,
    /// Each held cell as it is entered in Excel, for the notes.
    entered: BTreeMap<(u32, u32), String>,
    refuse: bool,
    reads: Vec<GridArea>,
    visited: usize,
}

impl TestSheet {
    /// A sheet holding these cells, each given as its A1 address, its Excel entry and its value.
    fn new(cells: &[(&str, &str, CalcValue)]) -> Self {
        let mut sheets: IdAllocator<SheetId> = IdAllocator::new();
        let id = sheets.issue().expect("a sheet");
        let mut held = BTreeMap::new();
        let mut entered = BTreeMap::new();
        for (address, entry, value) in cells {
            assert!(held.insert(decode_a1(address), value.clone()).is_none());
            entered.insert(decode_a1(address), format!("{address} = {entry}"));
        }
        TestSheet {
            id,
            cells: held,
            entered,
            refuse: false,
            reads: Vec::new(),
            visited: 0,
        }
    }

    fn key(&self, a1: &str) -> CellKey {
        let (row, column) = decode_a1(a1);
        CellKey::new(self.id, row, column).unwrap_or_else(|e| panic!("{a1}: {e:?}"))
    }

    /// The area an A1 text names: a cell, a range between two cells, or a whole column (A:A).
    fn area(&self, text: &str) -> GridArea {
        match text.split_once(':') {
            None => GridArea::cell(self.key(text)),
            Some((first, last)) if first.bytes().all(|b| b.is_ascii_uppercase()) => GridArea::new(
                self.key(&format!("{first}1")),
                self.key(&format!("{last}{MAX_ROWS}")),
            )
            .expect("a whole column"),
            Some((first, last)) => GridArea::new(self.key(first), self.key(last)).expect("a range"),
        }
    }

    /// The Excel entries of the held cells of these areas, or that they are blank.
    fn entries(&self, texts: &[&str]) -> Vec<String> {
        let areas: Vec<GridArea> = texts.iter().map(|text| self.area(text)).collect();
        let held: Vec<String> = self
            .entered
            .iter()
            .filter(|((row, column), _)| {
                let key = CellKey::new(self.id, *row, *column).expect("a held cell");
                areas.iter().any(|area| area.contains(key))
            })
            .map(|(_, entry)| entry.clone())
            .collect();
        if held.is_empty() {
            vec![format!("{} blank", texts.join(","))]
        } else {
            held
        }
    }

    /// The whole sheet as it is entered in Excel.
    fn described(&self) -> String {
        self.entered
            .values()
            .cloned()
            .collect::<Vec<_>>()
            .join(", ")
    }

    /// What a value parameter receives for a cell: the caller dereferences it to its value, or to
    /// Empty when it is blank.
    fn value(&self, a1: &str) -> CalcValue {
        self.cells
            .get(&decode_a1(a1))
            .cloned()
            .unwrap_or_else(CalcValue::empty)
    }
}

impl CellReader for TestSheet {
    fn subtotal_exclusions(
        &mut self,
        _: calc_func::value::GridArea,
    ) -> Option<calc_func::SubtotalExclusions> {
        None
    }

    fn read_area(
        &mut self,
        area: GridArea,
        visit: &mut dyn FnMut(CellKey, &CalcValue) -> ControlFlow<()>,
    ) -> Result<(), Unreadable> {
        self.reads.push(area);
        if self.refuse {
            return Err(Unreadable);
        }
        let mut visited = 0;
        // The map is ordered by row and then column, so this walk is row-major.
        for (&(row, column), value) in &self.cells {
            let key = CellKey::new(self.id, row, column).expect("a held cell");
            if !area.contains(key) {
                continue;
            }
            visited += 1;
            if visit(key, value).is_break() {
                break;
            }
        }
        self.visited += visited;
        Ok(())
    }
}

/// One argument of a call: as an Excel formula writes it, and as the caller supplies it.
#[derive(Clone)]
struct Arg {
    excel: String,
    value: CalcValue,
    /// The cells of the test sheet it reads, as the notes list them.
    reads: Vec<String>,
}

fn arg(excel: &str, value: CalcValue) -> Arg {
    Arg {
        excel: excel.to_owned(),
        value,
        reads: Vec::new(),
    }
}

/// A number literal.
fn num(literal: &str) -> Arg {
    let value: f64 = literal.parse().expect("a number literal");
    arg(literal, CalcValue::number(value))
}

/// A text literal.
fn txt(text: &str) -> Arg {
    arg(
        &format!("\"{}\"", text.replace('"', "\"\"")),
        CalcValue::text(ExcelText::from_interop_assignment(text)),
    )
}

fn logical(value: bool) -> Arg {
    arg(
        if value { "TRUE" } else { "FALSE" },
        CalcValue::logical(value),
    )
}

/// An error literal.
fn err(code: WorksheetErrorCode) -> Arg {
    arg(spelled(code), CalcValue::error(code))
}

/// An omitted argument.
fn omitted() -> Arg {
    arg("", CalcValue::missing())
}

/// A cell given to a value parameter, which the caller dereferences.
fn cell(sheet: &TestSheet, a1: &str) -> Arg {
    Arg {
        reads: sheet.entries(&[a1]),
        ..arg(a1, sheet.value(a1))
    }
}

/// A reference to the areas of the test sheet that these A1 texts name, given as a reference.
fn reference(sheet: &TestSheet, areas: &[&str]) -> Arg {
    let excel = match areas {
        [one] => (*one).to_owned(),
        _ => format!("({})", areas.join(",")),
    };
    let value = ReferenceLike::from_areas(areas.iter().map(|a| sheet.area(a)).collect())
        .expect("areas of one sheet");
    Arg {
        reads: sheet.entries(areas),
        ..arg(&excel, CalcValue::reference(value))
    }
}

fn text_value(text: &str) -> CalcValue {
    CalcValue::text(ExcelText::from_interop_assignment(text))
}

/// The error literals the fixtures write.
const ERROR_LITERALS: [(WorksheetErrorCode, &str); 8] = [
    (WorksheetErrorCode::Null, "#NULL!"),
    (WorksheetErrorCode::Div0, "#DIV/0!"),
    (WorksheetErrorCode::Value, "#VALUE!"),
    (WorksheetErrorCode::Ref, "#REF!"),
    (WorksheetErrorCode::Name, "#NAME?"),
    (WorksheetErrorCode::Num, "#NUM!"),
    (WorksheetErrorCode::NA, "#N/A"),
    (WorksheetErrorCode::GettingData, "#GETTING_DATA"),
];

fn spelled(code: WorksheetErrorCode) -> &'static str {
    ERROR_LITERALS
        .iter()
        .find(|(known, _)| *known == code)
        .map(|(_, literal)| *literal)
        .unwrap_or_else(|| panic!("{code:?} has no literal in these fixtures"))
}

/// What a call is expected to give.
#[derive(Clone, Debug)]
enum Expect {
    /// A number, compared bit for bit.
    Number(f64),
    Logical(bool),
    Text(String),
    /// Text given as its UTF-16 code units, for a text that no Rust string holds (an unpaired
    /// surrogate).
    Units(Vec<u16>),
    Error(WorksheetErrorCode),
    /// `FunctionFault::NotImplemented` under the entry's own ID, naming this argument.
    Refused(Option<usize>),
    /// `FunctionFault::ReferenceRequired` under the entry's own ID, naming this argument.
    Required(usize),
}

use Expect::{Error as E, Logical as L, Number as N, Refused};

fn t(text: &str) -> Expect {
    Expect::Text(text.to_owned())
}

impl Expect {
    fn holds(&self, id: FunctionId, result: &Result<CalcValue, FunctionFault>) -> bool {
        match (self, result) {
            (N(n), Ok(value)) => value.as_number().map(f64::to_bits) == Some(n.to_bits()),
            (L(b), Ok(value)) => value.as_logical() == Some(*b),
            (Expect::Text(text), Ok(value)) => {
                value.as_text() == Some(&ExcelText::from_interop_assignment(text))
            }
            (Expect::Units(units), Ok(value)) => {
                value.as_text().map(ExcelText::utf16_code_units) == Some(units.as_slice())
            }
            (E(code), Ok(value)) => {
                value.as_error() == Some(*code) && value.callable_value().is_none()
            }
            (
                Refused(argument),
                Err(FunctionFault::NotImplemented {
                    id: refused,
                    argument: at,
                }),
            ) => *refused == id && at == argument,
            (
                Expect::Required(argument),
                Err(FunctionFault::ReferenceRequired { id: refused, index }),
            ) => *refused == id && index == argument,
            _ => false,
        }
    }

    fn shown(&self) -> String {
        match self {
            N(n) => format!("number {n:?} (bits {:#018x})", n.to_bits()),
            L(b) => (if *b { "TRUE" } else { "FALSE" }).to_owned(),
            Expect::Text(text) if text.len() > 24 => {
                format!("text of {} characters", text.len())
            }
            Expect::Text(text) => format!("text {text:?}"),
            Expect::Units(units) => format!(
                "text of {} UTF-16 code units ending {:#06x}",
                units.len(),
                units.last().copied().unwrap_or_default()
            ),
            E(code) => spelled(*code).to_owned(),
            Refused(argument) => format!("NotImplemented (argument {argument:?})"),
            Expect::Required(argument) => format!("ReferenceRequired (argument {argument})"),
        }
    }
}

/// A value as a note or a problem shows it.
fn shown_value(value: &CalcValue) -> String {
    match value.core() {
        CoreValue::Number(n) => format!("number {n:?} (bits {:#018x})", n.to_bits()),
        CoreValue::Logical(b) => (if *b { "TRUE" } else { "FALSE" }).to_owned(),
        CoreValue::Text(text) if text.len_utf16_code_units() > 24 => {
            format!("text of {} UTF-16 code units", text.len_utf16_code_units())
        }
        CoreValue::Text(text) => format!("text {:?}", text.to_string_lossy()),
        CoreValue::Error(code) => ERROR_LITERALS
            .iter()
            .find(|(known, _)| known == code)
            .map_or_else(|| format!("{code:?}"), |(_, literal)| (*literal).to_owned()),
        other => format!("{other:?}"),
    }
}

/// Whether two values are the same value, numbers bit for bit. It checks the fixtures' own
/// consistency (the test sheet against the oracle workbook's constant cells, an Exact call against
/// its Fast row); the comparison with Excel is calc_verdict::judge's alone.
fn identical(a: &CalcValue, b: &CalcValue) -> bool {
    // A NaN number (Excel's bit pattern past DBL_MAX) is identical to the same bits.
    let both_nan = a.as_number().is_some_and(f64::is_nan) && b.as_number().is_some_and(f64::is_nan);
    (a == b || both_nan) && a.as_number().map(f64::to_bits) == b.as_number().map(f64::to_bits)
}

/// The bead that owns the Excel behaviour a row shows, when that is not cfunc-m1.2, and the
/// behaviour. Such a row is published to the Excel comparison as not modeled, with this owner.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Owner {
    bead: &'static str,
    behaviour: &'static str,
}

impl Owner {
    /// The not-modeled feature the row is published with.
    fn feature(self) -> String {
        format!("{}: {}", self.bead, self.behaviour)
    }
}

/// Wildcard criteria over text beyond ASCII, which calc_func's pattern matching refuses.
const COLLATION: Owner = Owner {
    bead: "dc-uar",
    behaviour: "wildcard criteria over text beyond ASCII, as Excel matches them",
};
const NEAR_ZERO: Owner = Owner {
    bead: "dc-6qki",
    behaviour: "the near-zero adjustment of + and - by the formula's final operation",
};
/// Text that holds a digit and no word that rules out a number, or that holds a character beyond
/// printable ASCII (dc-e00t's verdict decides the rest), read as a number or a logical.
const NUMBER_TEXT: Owner = Owner {
    bead: "dc-uar",
    behaviour: "text holding a digit and no word that rules out a number, or a character beyond printable ASCII, read as a number or a logical by Excel's number-text rules",
};
/// Excel loads the oracle workbook's IF({TRUE,FALSE},1,2) as the legacy formula
/// =@IF({TRUE,FALSE},1,2) and gives 1, so its row measures that legacy formula, not IF lifted over
/// the array; calc_func refuses the array condition.
const LEGACY_ARRAY_IF: Owner = Owner {
    bead: "dc-10y",
    behaviour: "an array condition in a legacy formula, which Excel loads as =@IF({TRUE,FALSE},1,2)",
};
/// COUNT and COUNTIF read number text through cfunc-m1.2's coercion, but each is another bead's
/// function, so their rows here leave cfunc-m1.2's own count of matches.
const COUNT_TEXT: Owner = Owner {
    bead: "cfunc-m3.4",
    behaviour: "number text read by COUNT",
};
const CRITERIA_TEXT: Owner = Owner {
    bead: "cfunc-m3.5",
    behaviour: "number text in COUNTIF's criteria and range cells",
};

/// An oracle row: its formula's cell in the oracle workbook (column D of its case's sheet, at row
/// `row`), the formula, what calc_func::invoke returned at its call's tick, the choice it encodes,
/// if any, and the bead that owns its behaviour when that is not cfunc-m1.2.
struct OracleRow {
    row: u32,
    /// As `FunctionCallTarget::oracle_formula` writes it, with its leading `=`.
    formula: String,
    outcome: Result<CalcValue, FunctionFault>,
    choice: Option<&'static str>,
    owner: Option<Owner>,
    tick: u64,
}

/// An invoke call recorded at its call site: the row whose formula it evaluated and its tick. The
/// evaluation record is built from these, apart from the results the rows publish.
struct Call {
    row: u32,
    tick: u64,
}

/// The rows a case ran, in order.
struct Rows {
    case: &'static str,
    /// The oracle workbook's sheet that holds this case's formulas.
    sheet: &'static str,
    oracle: Vec<OracleRow>,
    calls: Vec<Call>,
    /// Prints no notes (the rows of the other sheet, run again for the Excel comparison).
    quiet: bool,
    /// The rows [`basis_row`] ran on a [`Basis::Measured`] rule.
    measured: usize,
}

impl Rows {
    fn new(case: &'static str, sheet: &'static str, quiet: bool) -> Self {
        Rows {
            case,
            sheet,
            oracle: Vec::new(),
            calls: Vec::new(),
            quiet,
            measured: 0,
        }
    }

    fn choices(&self) -> usize {
        self.oracle
            .iter()
            .filter(|row| row.choice.is_some())
            .count()
    }

    /// The rows whose behaviour another bead owns.
    fn elsewhere(&self) -> usize {
        self.oracle.iter().filter(|row| row.owner.is_some()).count()
    }

    /// The rows calc-func refuses.
    fn refused(&self) -> usize {
        self.oracle
            .iter()
            .filter(|row| row.outcome.is_err())
            .count()
    }
}

thread_local! {
    /// The oracle rows' invoke calls made on this thread so far: the last call's tick.
    static TICKS: Cell<u64> = const { Cell::new(0) };
}

/// The tick of the next invoke call.
fn next_tick() -> u64 {
    TICKS.with(|ticks| {
        let tick = ticks.get() + 1;
        ticks.set(tick);
        tick
    })
}

/// Runs the entry written `spelling` in `notation` on `args` through `calc_func::invoke`, with the
/// test sheet as the services' cell reader, and asserts `expect`. Returns the Excel formula of
/// the call, the arguments the call interface asked for, in order, the result and the call's
/// tick.
fn call_and_assert(
    sheet: &mut TestSheet,
    mode: &NumericMode,
    (notation, spelling): (Notation, &str),
    args: &[Arg],
    expect: &Expect,
) -> (String, Vec<usize>, Result<CalcValue, FunctionFault>, u64) {
    let target = surface(notation, spelling);
    let texts: Vec<&str> = args.iter().map(|a| a.excel.as_str()).collect();
    let formula = target
        .oracle_formula(&texts)
        .unwrap_or_else(|| panic!("{notation:?} {spelling:?} does not admit {texts:?}"));
    let mut recorded = Recorded::new(args.iter().map(|a| a.value.clone()).collect());
    let tick = next_tick();
    let result = invoke(
        target,
        &mut recorded,
        &mut EvalServices::with_cells(mode, sheet),
        &mut FunctionCallScratch::new(),
    );
    assert!(
        expect.holds(target.id(), &result),
        "{formula} under {mode:?}: expected {}, got {result:?}",
        expect.shown()
    );
    (formula, recorded.asked, result, tick)
}

/// [`call_and_assert`] under Fast, then records the call and the row, with the next cell of
/// column D, and prints the row: as an `oracle` line when its behaviour is cfunc-m1.2's, which
/// must then compute a value, and as a `not modeled here` line naming `owner` otherwise. Returns
/// the arguments the call interface asked for.
fn check(
    rows: &mut Rows,
    sheet: &mut TestSheet,
    surface: (Notation, &str),
    args: &[Arg],
    expect: Expect,
    choice: Option<&'static str>,
    owner: Option<Owner>,
) -> Vec<usize> {
    let (formula, asked, result, tick) =
        call_and_assert(sheet, &NumericMode::Fast, surface, args, &expect);
    let reads: Vec<&str> = args
        .iter()
        .flat_map(|a| a.reads.iter().map(String::as_str))
        .collect();
    let cells = if reads.is_empty() {
        String::new()
    } else {
        format!(" with {}", reads.join(", "))
    };
    match owner {
        None => assert!(
            result.is_ok(),
            "{formula}: a row of cfunc-m1.2 computes a value; a refused row names the bead that owns its behaviour"
        ),
        Some(_) => assert!(
            choice.is_none(),
            "{formula}: a row another bead owns encodes no choice of cfunc-m1.2"
        ),
    }
    let row = u32::try_from(rows.oracle.len() + 1).expect("a row number");
    if !rows.quiet {
        let case = rows.case;
        let at = format!("{}!{}", rows.sheet, encode_a1(row, FORMULA_COLUMN));
        let shown = expect.shown();
        match owner {
            None => {
                let label = match (captured(), choice) {
                    (true, Some(choice)) => {
                        format!("compared with Excel's capture below; the rule it shows: {choice}")
                    }
                    (true, None) => "compared with Excel's capture below".to_owned(),
                    (false, Some(choice)) => format!("unmeasured choice: {choice}"),
                    (false, None) => "unmeasured".to_owned(),
                };
                note!(
                    "ACCEPT-NOTE: {case}: oracle {at} {formula}{cells} -> {shown} (hand-written, asserted as the code's own consistency check; {label})"
                );
            }
            Some(owner) => note!(
                "ACCEPT-NOTE: {case}: not modeled here, owned by {}: {at} {formula}{cells} -> {shown} (hand-written, asserted as the code's own consistency check; published as not modeled: {})",
                owner.bead,
                owner.behaviour
            ),
        }
    }
    rows.calls.push(Call { row, tick });
    rows.oracle.push(OracleRow {
        row,
        formula,
        outcome: result,
        choice,
        owner,
        tick,
    });
    asked
}

/// Whether the committed capture is Excel's capture of the oracle workbook rather than the
/// absent-capture marker; [`compare_with_excel`] then compares every row with it.
fn captured() -> bool {
    MissingCapture::recognize(ORACLE_CAPTURE).is_none()
}

/// Runs a row of cfunc-m1.2: `row!(surface, [args], expect)`, or with a fourth argument naming
/// the rule the row shows. Evaluates to the arguments the call interface asked for.
macro_rules! row {
    ($rows:expr, $sheet:expr, $surface:expr, [$($arg:expr),* $(,)?], $expect:expr $(, $choice:expr)?) => {{
        let args = [$($arg),*];
        let choice: Option<&'static str> = None $(.or(Some($choice)))?;
        check($rows, $sheet, $surface, &args, $expect, choice, None)
    }};
}

/// Runs a row whose behaviour another bead owns: `owned_by!(owner, surface, [args], expect)`.
macro_rules! owned_by {
    ($rows:expr, $sheet:expr, $owner:expr, $surface:expr, [$($arg:expr),* $(,)?], $expect:expr) => {{
        let args = [$($arg),*];
        check($rows, $sheet, $surface, &args, $expect, None, Some($owner))
    }};
}

const PLUS: (Notation, &str) = (Notation::Infix, "+");
const MINUS: (Notation, &str) = (Notation::Infix, "-");
const TIMES: (Notation, &str) = (Notation::Infix, "*");
const DIVIDE: (Notation, &str) = (Notation::Infix, "/");
const POWER: (Notation, &str) = (Notation::Infix, "^");
const NEGATE: (Notation, &str) = (Notation::Prefix, "-");
const UNARY_PLUS: (Notation, &str) = (Notation::Prefix, "+");
const PERCENT: (Notation, &str) = (Notation::Postfix, "%");
const EQ: (Notation, &str) = (Notation::Infix, "=");
const NE: (Notation, &str) = (Notation::Infix, "<>");
const LT: (Notation, &str) = (Notation::Infix, "<");
const LE: (Notation, &str) = (Notation::Infix, "<=");
const GT: (Notation, &str) = (Notation::Infix, ">");
const GE: (Notation, &str) = (Notation::Infix, ">=");
const AMPERSAND: (Notation, &str) = (Notation::Infix, "&");
const SUM: (Notation, &str) = (Notation::Call, "SUM");
const IF: (Notation, &str) = (Notation::Call, "IF");
const COUNT: (Notation, &str) = (Notation::Call, "COUNT");
const COUNTIF: (Notation, &str) = (Notation::Call, "COUNTIF");

fn exact_mode() -> NumericMode {
    NumericMode::Exact(
        NumericTargetId::new(
            "test-build",
            "test-channel",
            "test-compatibility",
            "test-environment",
        )
        .expect("a target"),
    )
}

// ---------------------------------------------------------------------------------------------
// The oracle workbook and its capture.

/// The parts of a ZIP package whose entries are stored (not compressed), as the generator writes
/// them, by name.
fn stored_parts(package: &[u8]) -> BTreeMap<String, &[u8]> {
    let u16_at = |at: usize| usize::from(u16::from_le_bytes([package[at], package[at + 1]]));
    let u32_at = |at: usize| {
        u32::from_le_bytes([
            package[at],
            package[at + 1],
            package[at + 2],
            package[at + 3],
        ]) as usize
    };
    let mut parts = BTreeMap::new();
    let mut at = 0;
    while at + 30 <= package.len() && u32_at(at) == 0x0403_4b50 {
        assert_eq!(
            u16_at(at + 8),
            0,
            "a part of the oracle workbook is compressed"
        );
        let size = u32_at(at + 18);
        let (name_length, extra_length) = (u16_at(at + 26), u16_at(at + 28));
        let name = &package[at + 30..at + 30 + name_length];
        let data = at + 30 + name_length + extra_length;
        let name = String::from_utf8(name.to_vec()).expect("an ASCII part name");
        assert!(
            parts.insert(name, &package[data..data + size]).is_none(),
            "a part repeats"
        );
        at = data + size;
    }
    assert_eq!(
        u32_at(at),
        0x0201_4b50,
        "the local entries of the oracle workbook end where its central directory starts"
    );
    parts
}

fn part_text<'p>(parts: &BTreeMap<String, &'p [u8]>, name: &str) -> &'p str {
    let bytes = parts
        .get(name)
        .unwrap_or_else(|| panic!("the oracle workbook has no part {name}"));
    std::str::from_utf8(bytes).unwrap_or_else(|e| panic!("{name}: {e}"))
}

/// The start tags `<name ...>` of an XML text.
fn start_tags<'x>(xml: &'x str, name: &str) -> Vec<&'x str> {
    let open = format!("<{name} ");
    let mut tags = Vec::new();
    let mut rest = xml;
    while let Some(at) = rest.find(&open) {
        rest = &rest[at..];
        let end = rest.find('>').expect("a start tag ends");
        tags.push(&rest[..end]);
        rest = &rest[end..];
    }
    tags
}

/// The value of attribute `name` of a start tag.
fn attribute<'x>(tag: &'x str, name: &str) -> Option<&'x str> {
    let marker = format!(" {name}=\"");
    let start = tag.find(&marker)? + marker.len();
    let end = tag[start..].find('"')? + start;
    Some(&tag[start..end])
}

fn unescape(text: &str) -> String {
    text.replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&apos;", "'")
        .replace("&amp;", "&")
}

/// A cell of an oracle sheet: a formula (its text, without `=`), a dynamic-array formula over its
/// one cell (a source formula), or a constant.
#[derive(Debug)]
enum FixtureCell {
    Formula(String),
    DynamicArray(String),
    Constant(CalcValue),
}

/// The cells of a worksheet part as the generator writes them: `<c r="..." [t="..."]>` holding an
/// `<f>`, a `<v>` or an inline string.
fn worksheet_cells(xml: &str) -> BTreeMap<(u32, u32), FixtureCell> {
    let mut cells = BTreeMap::new();
    let mut rest = xml;
    while let Some(at) = rest.find("<c ") {
        rest = &rest[at..];
        let tag_end = rest.find('>').expect("a cell's start tag ends");
        let tag = &rest[..tag_end];
        assert!(!tag.ends_with('/'), "an empty cell element: {tag}");
        let close = rest.find("</c>").expect("a cell ends");
        let body = &rest[tag_end + 1..close];
        rest = &rest[close + 4..];
        let inner = |name: &str| -> Option<String> {
            let open = format!("<{name}>");
            let start = body.find(&open)? + open.len();
            let end = body[start..].find(&format!("</{name}>"))? + start;
            Some(unescape(&body[start..end]))
        };
        let reference = attribute(tag, "r").expect("a cell names its reference");
        let value =
            |name: &str| inner(name).unwrap_or_else(|| panic!("{reference} has no <{name}>"));
        // A cell with cell metadata holds a dynamic-array formula over its one cell.
        let dynamic = attribute(tag, "cm").map(|metadata| {
            assert_eq!(metadata, "1", "{reference}: cell metadata {metadata}");
            let formula = body
                .strip_prefix(&format!("<f t=\"array\" ref=\"{reference}\">"))
                .and_then(|rest| rest.strip_suffix("</f>"))
                .unwrap_or_else(|| {
                    panic!("{reference}: no dynamic-array formula over its one cell: {body}")
                });
            FixtureCell::DynamicArray(unescape(formula))
        });
        let cell = dynamic.unwrap_or_else(|| match (inner("f"), attribute(tag, "t")) {
            (Some(formula), None) => FixtureCell::Formula(formula),
            (Some(_), Some(t)) => panic!("{reference}: a formula cell of type {t}"),
            (None, None | Some("n")) => {
                FixtureCell::Constant(CalcValue::number(value("v").parse().expect("a number")))
            }
            (None, Some("b")) => FixtureCell::Constant(CalcValue::logical(value("v") == "1")),
            (None, Some("e")) => {
                let literal = value("v");
                let code = ERROR_LITERALS
                    .iter()
                    .find(|(_, known)| *known == literal)
                    .unwrap_or_else(|| panic!("{reference}: {literal} is no error literal"))
                    .0;
                FixtureCell::Constant(CalcValue::error(code))
            }
            (None, Some("inlineStr")) => FixtureCell::Constant(text_value(&value("t"))),
            (None, Some(other)) => panic!("{reference}: a cell of type {other}"),
        });
        assert!(
            cells.insert(decode_a1(reference), cell).is_none(),
            "{reference} repeats"
        );
    }
    cells
}

/// A formula as SpreadsheetML stores it: a function added to Excel after the file format's first
/// version is written with the `_xlfn.` prefix, which Excel requires to recognize it in a file
/// (MAXIFS, MINIFS, UNICHAR, XOR and IFNA among the functions these fixtures call). XOR and IFNA are
/// matched as whole names, so a longer name that ends in one, such as BITXOR, keeps its own
/// spelling.
fn stored_formula(formula: &str) -> String {
    let mut stored = formula
        .replace("MAXIFS(", "_xlfn.MAXIFS(")
        .replace("MINIFS(", "_xlfn.MINIFS(")
        .replace("UNICHAR(", "_xlfn.UNICHAR(");
    for name in ["XOR(", "IFNA("] {
        let mut prefixed = String::with_capacity(stored.len());
        let mut from = 0;
        for (at, _) in stored.match_indices(name) {
            let whole_name = stored[..at].chars().next_back().is_none_or(|before| {
                !(before.is_ascii_alphanumeric() || matches!(before, '_' | '.'))
            });
            if whole_name {
                prefixed.push_str(&stored[from..at]);
                prefixed.push_str("_xlfn.");
                from = at;
            }
        }
        prefixed.push_str(&stored[from..]);
        stored = prefixed;
    }
    stored
}

/// Asserts that the oracle workbook's sheet for this case holds exactly the test sheet's cells
/// and the case's oracle formulas, each in column D at its row, and nothing else.
fn assert_oracle_sheet(rows: &Rows, sheet: &TestSheet) {
    assert_workbook_sheet(ORACLE_WORKBOOK, ORACLE_WORKBOOK_PATH, rows, sheet);
}

/// [`assert_oracle_sheet`] for the committed workbook `package`, at `path`.
fn assert_workbook_sheet(package: &[u8], path: &str, rows: &Rows, sheet: &TestSheet) {
    assert_workbook_sheet_with_sources(package, path, rows, sheet, &BTreeMap::new());
}

/// [`assert_workbook_sheet`] for a sheet that also holds source formulas: `sources` gives each
/// one's place and its formula as the file stores it, held there as a dynamic-array formula over
/// its one cell, where the test sheet holds the value its reader gives for that cell.
fn assert_workbook_sheet_with_sources(
    package: &[u8],
    path: &str,
    rows: &Rows,
    sheet: &TestSheet,
    sources: &BTreeMap<(u32, u32), String>,
) {
    let parts = stored_parts(package);
    let workbook = part_text(&parts, "xl/workbook.xml");
    let relationships = part_text(&parts, "xl/_rels/workbook.xml.rels");
    let id = start_tags(workbook, "sheet")
        .into_iter()
        .find(|tag| attribute(tag, "name") == Some(rows.sheet))
        .and_then(|tag| attribute(tag, "r:id"))
        .unwrap_or_else(|| panic!("the oracle workbook lists no sheet {}", rows.sheet));
    let target = start_tags(relationships, "Relationship")
        .into_iter()
        .find(|tag| attribute(tag, "Id") == Some(id))
        .and_then(|tag| attribute(tag, "Target"))
        .unwrap_or_else(|| panic!("the oracle workbook has no relationship {id}"));
    let cells = worksheet_cells(part_text(&parts, &format!("xl/{target}")));

    let mut formulas = BTreeMap::new();
    let mut dynamic = BTreeMap::new();
    let mut constants = BTreeMap::new();
    for (place, cell) in cells {
        match cell {
            FixtureCell::Formula(formula) => {
                formulas.insert(place, formula);
            }
            FixtureCell::DynamicArray(formula) => {
                dynamic.insert(place, formula);
            }
            FixtureCell::Constant(value) => {
                constants.insert(place, value);
            }
        }
    }
    let expected: BTreeMap<(u32, u32), String> = rows
        .oracle
        .iter()
        .map(|row| ((row.row, FORMULA_COLUMN), stored_formula(&row.formula[1..])))
        .collect();
    let at = |(row, column): (u32, u32)| format!("{}!{}", rows.sheet, encode_a1(row, column));
    let mut differences = Vec::new();
    for (place, formula) in &expected {
        match formulas.get(place) {
            Some(found) if found == formula => {}
            found => differences.push(format!(
                "{}: the row's formula is {formula:?}, the workbook holds {found:?}",
                at(*place)
            )),
        }
    }
    for place in formulas
        .keys()
        .filter(|place| !expected.contains_key(place))
    {
        differences.push(format!("{}: a formula that no oracle row has", at(*place)));
    }
    if dynamic != *sources {
        differences.push(format!(
            "the sheet's dynamic-array formulas are {dynamic:?}, the source formulas {sources:?}"
        ));
    }
    for (place, value) in &sheet.cells {
        match constants.get(place) {
            _ if sources.contains_key(place) => {}
            Some(found) if identical(found, value) => {}
            found => differences.push(format!(
                "{}: the test sheet holds {value:?}, the workbook {found:?}",
                at(*place)
            )),
        }
    }
    for place in constants
        .keys()
        .filter(|place| !sheet.cells.contains_key(place))
    {
        differences.push(format!(
            "{}: a constant the test sheet does not hold",
            at(*place)
        ));
    }
    assert!(
        differences.is_empty(),
        "the oracle workbook's sheet {} differs from the case's rows and cells:\n{}",
        rows.sheet,
        differences.join("\n")
    );
    if !rows.quiet && sources.is_empty() {
        note!(
            "ACCEPT-NOTE: {}: {path} (sha256 {}): sheet {} holds exactly the {} oracle rows' formulas in column D, in row order, and the {} cells the rows read",
            rows.case,
            sha256_hex(package),
            rows.sheet,
            expected.len(),
            sheet.cells.len()
        );
    } else if !rows.quiet {
        note!(
            "ACCEPT-NOTE: {}: {path} (sha256 {}): sheet {} holds exactly the {} oracle rows' formulas in column D, in row order, the {} constant cells the rows read and the {} source formulas, each as a dynamic-array formula over its one cell (cell metadata naming XLDAPR)",
            rows.case,
            sha256_hex(package),
            rows.sheet,
            expected.len(),
            sheet.cells.len() - sources.len(),
            sources.len()
        );
    }
}

/// Runs the committed generator under Windows PowerShell into a new directory and returns what it
/// wrote, by path relative to that directory. Each call has a directory of its own, so cases that
/// run the generator at the same time in one test process do not share one.
#[cfg(all(windows, not(target_arch = "wasm32")))]
fn generated_fixtures() -> BTreeMap<String, Vec<u8>> {
    use std::path::Path;
    use std::process::{Command, Stdio};
    use std::sync::atomic::{AtomicU32, Ordering};
    static RUNS: AtomicU32 = AtomicU32::new(0);
    let generator = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("fixtures")
        .join("make-fixtures.ps1");
    let out = std::env::temp_dir().join(format!(
        "calc-func-fixtures-{}-{}",
        std::process::id(),
        RUNS.fetch_add(1, Ordering::Relaxed)
    ));
    let _ = std::fs::remove_dir_all(&out);
    let system_root = std::env::var_os("SystemRoot").expect("SystemRoot is set on Windows");
    let powershell = Path::new(&system_root)
        .join("System32")
        .join("WindowsPowerShell")
        .join("v1.0")
        .join("powershell.exe");
    let ran = Command::new(&powershell)
        .args([
            "-NoLogo",
            "-NoProfile",
            "-NonInteractive",
            "-ExecutionPolicy",
            "Bypass",
            "-File",
        ])
        .arg(&generator)
        .arg("-OutDir")
        .arg(&out)
        .stdin(Stdio::null())
        .output()
        .expect("Windows PowerShell starts");
    assert!(
        ran.status.success(),
        "the generator failed ({:?}):\n{}\n{}",
        ran.status,
        String::from_utf8_lossy(&ran.stdout),
        String::from_utf8_lossy(&ran.stderr)
    );
    let mut written = BTreeMap::new();
    let mut pending = vec![out.clone()];
    while let Some(directory) = pending.pop() {
        for entry in std::fs::read_dir(&directory).expect("the generator's directory") {
            let path = entry.expect("a directory entry").path();
            if path.is_dir() {
                pending.push(path);
            } else {
                let name = path
                    .strip_prefix(&out)
                    .expect("under the generator's directory")
                    .to_string_lossy()
                    .replace('\\', "/");
                written.insert(name, std::fs::read(&path).expect("a written file"));
            }
        }
    }
    let _ = std::fs::remove_dir_all(&out);
    written
}

/// The authored revision the cases calculate: the oracle workbook as committed, with no authored
/// operation applied, which a capture of the fixture as authored records as revision 0.
const AUTHORED_REVISION: u64 = 0;

/// The calculation context the cases calculate under: the one the oracle workbook declares in its
/// `workbookPr` (date1904) and `calcPr` (iterate, iterateCount, iterateDelta, fullPrecision), an
/// absent element or attribute at its SpreadsheetML default. No row's value depends on these
/// settings; a capture taken under another context leaves every calculated row stale.
fn workbook_context() -> CalculationContext {
    package_context(ORACLE_WORKBOOK)
}

/// [`workbook_context`] of the oracle workbook `package`.
fn package_context(package: &[u8]) -> CalculationContext {
    let parts = stored_parts(package);
    let workbook = part_text(&parts, "xl/workbook.xml");
    let element = |name: &str| -> Option<&str> {
        let tags = start_tags(workbook, name);
        assert!(
            tags.len() <= 1,
            "the oracle workbook has {} {name} elements",
            tags.len()
        );
        tags.first().copied()
    };
    let (properties, calculation) = (element("workbookPr"), element("calcPr"));
    let flag = |tag: Option<&str>, name: &str, default: bool| match tag
        .and_then(|tag| attribute(tag, name))
    {
        None => default,
        Some("1" | "true") => true,
        Some("0" | "false") => false,
        Some(other) => panic!("{name}={other:?} is not a boolean"),
    };
    let number = |name: &str| calculation.and_then(|tag| attribute(tag, name));
    CalculationContext {
        date1904: flag(properties, "date1904", false),
        iteration: flag(calculation, "iterate", false),
        max_iterations: number("iterateCount").map_or(100, |n| n.parse().expect("a count")),
        max_change: number("iterateDelta").map_or(0.001, |n| n.parse().expect("a number")),
        precision_as_displayed: !flag(calculation, "fullPrecision", true),
    }
}

/// The cell of row `row`'s formula on `sheet` of the oracle workbook.
fn oracle_cell(sheet: &str, row: u32) -> CellAddress {
    CellAddress::new(sheet, row, FORMULA_COLUMN).expect("a cell of the grid")
}

/// What a run of the oracle rows of every sheet publishes: a result for each row's cell
/// (calculated at its call's tick for a row of cfunc-m1.2, not modeled with its owner for a row of
/// another bead), and the evaluation record of the invoke calls the rows recorded at their call
/// sites.
fn published(run: &[&Rows]) -> (ResultSet, EvaluationRecord) {
    published_in(ORACLE_WORKBOOK, CAPTURE_CASE, run)
}

/// [`published`] for the oracle workbook `package`, whose capture is recorded under `capture_case`.
fn published_in(
    package: &[u8],
    capture_case: &str,
    run: &[&Rows],
) -> (ResultSet, EvaluationRecord) {
    let source = format!("sha256:{}", sha256_hex(package));
    let context = package_context(package).fingerprint();
    let mut results = Vec::new();
    let mut evaluations = Vec::new();
    for rows in run {
        for row in &rows.oracle {
            let (provenance, value) = match (row.owner, &row.outcome) {
                (None, Ok(value)) => (
                    Provenance::Calculated {
                        tick: row.tick,
                        revision: AUTHORED_REVISION,
                        context: context.clone(),
                        external_links: false,
                    },
                    Some(value.clone()),
                ),
                (None, Err(fault)) => (
                    Provenance::Failed {
                        fault: format!("{fault:?}"),
                    },
                    None,
                ),
                (Some(owner), outcome) => (
                    Provenance::NotModeled {
                        feature: owner.feature(),
                        diagnostic: match outcome {
                            Ok(value) => format!("calc_func::invoke gives {}", shown_value(value)),
                            Err(fault) => format!("calc_func::invoke refuses: {fault:?}"),
                        },
                    },
                    None,
                ),
            };
            results.push(CellResult {
                cell: oracle_cell(rows.sheet, row.row),
                provenance,
                value,
            });
        }
        for call in &rows.calls {
            evaluations.push(Evaluation {
                cell: oracle_cell(rows.sheet, call.row),
                tick: call.tick,
                revision: AUTHORED_REVISION,
                context: context.clone(),
            });
        }
    }
    (
        ResultSet {
            case: capture_case.to_owned(),
            source: source.clone(),
            run: RunIdentity {
                revision: AUTHORED_REVISION,
                context,
            },
            results,
        },
        EvaluationRecord {
            case: capture_case.to_owned(),
            source,
            evaluations,
        },
    )
}

/// A case's Excel comparison: the report calc_verdict::judge computes over the whole run, and
/// what on it disagrees with the case's rows.
struct Comparison {
    report: VerdictReport,
    problems: Vec<String>,
}

/// The Excel comparison of `case` against `capture`, from the results and evaluations that `run`
/// (the rows of every sheet, `case`'s among them) publishes. Refused exactly when judge refuses.
fn excel_comparison(
    capture: &CaptureRecord,
    run: &[&Rows],
    case: &Rows,
) -> Result<Comparison, JudgeError> {
    let (results, evaluations) = published(run);
    judged(capture, &results, &evaluations, case)
}

/// [`excel_comparison`] from a result set and an evaluation record as given. The rows run under
/// Fast, so the policy selects no Exact target and judge certifies no comparison strict-bit.
fn judged(
    capture: &CaptureRecord,
    results: &ResultSet,
    evaluations: &EvaluationRecord,
    case: &Rows,
) -> Result<Comparison, JudgeError> {
    let policy = VerdictPolicy {
        numeric_mode: NumericMode::Fast,
        evidence: None,
    };
    let report = judge(capture, results, evaluations, &policy)?;
    let problems = verdict_problems(&report, case);
    Ok(Comparison { report, problems })
}

/// Whether `owner` names the bead whose acceptance case `case` is, the case name's prefix: a
/// bead's own refusal is not modelled by that bead, so it never counts as owned elsewhere.
fn owned_by_own_bead(owner: Owner, case: &Rows) -> bool {
    let bead = case.case.split("__").next().expect("a case name");
    owner
        .bead
        .split(", ")
        .any(|named| named.replace(['-', '.'], "_") == bead)
}

/// What on `report` disagrees with the rows of `case`'s sheet: an inventory cell of the sheet that
/// no row has, a row whose owner is the case's own bead, a row of cfunc-m1.2 whose verdict is not
/// `matched`, a row of another bead whose verdict is not `not_modeled` with its owner, and a row
/// whose cell the inventory lacks.
fn verdict_problems(report: &VerdictReport, case: &Rows) -> Vec<String> {
    verdict_problems_beside(report, case, &[])
}

/// [`verdict_problems`] for a sheet that also holds the formula cells `beside` (row, column), which
/// no row has and whose verdicts the caller checks.
fn verdict_problems_beside(
    report: &VerdictReport,
    case: &Rows,
    beside: &[(u32, u32)],
) -> Vec<String> {
    let rows: BTreeMap<u32, &OracleRow> = case.oracle.iter().map(|row| (row.row, row)).collect();
    let mut problems = Vec::new();
    for cell in &report.cells {
        let address = &cell.formula.cell;
        if address.sheet() != case.sheet || beside.contains(&(address.row(), address.column())) {
            continue;
        }
        let at = format!("{}!{}", address.sheet(), address.a1());
        let row = match address.column() {
            FORMULA_COLUMN => rows.get(&address.row()),
            _ => None,
        };
        let Some(row) = row else {
            problems.push(format!(
                "{at}: a formula cell that no oracle row has ({})",
                cell.verdict.name()
            ));
            continue;
        };
        let formula = &row.formula;
        if let Some(owner) = row.owner.filter(|owner| owned_by_own_bead(*owner, case)) {
            problems.push(format!(
                "{at} {formula}: a row of {} names its own bead {} as the owner of what it refuses ({}): {:?}",
                case.case,
                owner.bead,
                cell.verdict.name(),
                cell.verdict
            ));
            continue;
        }
        match (row.owner, &cell.verdict) {
            (None, Verdict::Matched { .. }) => {}
            (Some(owner), Verdict::NotModeled { feature, .. }) if *feature == owner.feature() => {}
            (
                None,
                Verdict::Mismatched {
                    expected, actual, ..
                },
            ) => problems.push(format!(
                "{at} {formula}: calc_func::invoke gave {}, Excel's raw value is {}",
                shown_value(actual),
                shown_value(expected)
            )),
            (None, verdict) => problems.push(format!(
                "{at} {formula}: a row of {} is {}, not matched: {verdict:?}",
                case.case,
                verdict.name()
            )),
            (Some(owner), verdict) => problems.push(format!(
                "{at} {formula}: a row owned by {} is {}, not not_modeled with its owner: {verdict:?}",
                owner.bead,
                verdict.name()
            )),
        }
    }
    for outside in &report.outside_inventory {
        if outside.cell.sheet() == case.sheet {
            problems.push(format!(
                "{}!{}: the capture's inventory lacks this oracle row's cell",
                outside.cell.sheet(),
                outside.cell.a1()
            ));
        }
    }
    problems
}

/// The verdicts of `case`'s sheet on `report`, counted by name, with the not-modeled ones also
/// counted by owner, as a note shows them.
fn sheet_verdicts(report: &VerdictReport, case: &Rows) -> String {
    let mut names: BTreeMap<&str, usize> = BTreeMap::new();
    let mut owners: BTreeMap<&str, usize> = BTreeMap::new();
    for cell in &report.cells {
        if cell.formula.cell.sheet() != case.sheet {
            continue;
        }
        *names.entry(cell.verdict.name()).or_default() += 1;
        if let Verdict::NotModeled { feature, .. } = &cell.verdict {
            *owners.entry(feature.as_str()).or_default() += 1;
        }
    }
    let names: Vec<String> = names.iter().map(|(n, c)| format!("{n} {c}")).collect();
    let owners: Vec<String> = owners.iter().map(|(o, c)| format!("{c} ({o})")).collect();
    format!(
        "{}; not modeled by owner: {}",
        names.join(", "),
        if owners.is_empty() {
            "none".to_owned()
        } else {
            owners.join(", ")
        }
    )
}

/// Prints how many rows a case ran and whose behaviour each shows.
fn rows_note(rows: &Rows) {
    let elsewhere = rows.elsewhere();
    note!(
        "ACCEPT-NOTE: {}: {} rows through calc_func::invoke, each asserted against a hand-written expectation as the code's own consistency check and each with its formula on sheet {} of the oracle workbook: {} of cfunc-m1.2 ({} of them naming the rule they show), {}, and {} whose behaviour another bead owns ({} of them refused by calc-func)",
        rows.case,
        rows.oracle.len(),
        rows.sheet,
        rows.oracle.len() - elsewhere,
        rows.choices(),
        if captured() {
            "each compared with Excel's capture below"
        } else {
            "unmeasured until the owner's capture"
        },
        elsewhere,
        rows.refused()
    );
}

/// The Excel comparison of a case's oracle rows. While the capture is absent the case prints
/// ACCEPT-NOT-RUN with the marker's MissingCapture. Otherwise the capture must be a live capture
/// of the committed oracle workbook by the committed worker, without isolation failures, and the
/// report calc_verdict::judge computes from it, from the results the rows of both sheets publish
/// and from their recorded invoke calls must hold no problem for the case's sheet
/// ([`verdict_problems`]). `other` runs the other sheet's rows.
fn compare_with_excel(rows: &Rows, other: fn(bool) -> Rows) {
    let case = rows.case;
    if let Some(missing) = MissingCapture::recognize(ORACLE_CAPTURE) {
        assert_eq!(
            (
                missing.case.as_str(),
                missing.fixture.as_str(),
                missing.command.as_str()
            ),
            (CAPTURE_CASE, ORACLE_WORKBOOK_PATH, CAPTURE_COMMAND),
            "the absent-capture marker names this capture"
        );
        note!("ACCEPT-NOT-RUN: {case}: {missing}");
        return;
    }
    let capture = CaptureRecord::decode(ORACLE_CAPTURE).expect("the committed capture decodes");
    assert_eq!(capture.case, CAPTURE_CASE);
    assert_eq!(capture.acquisition, Acquisition::LiveCapture);
    let session = capture
        .session
        .as_ref()
        .expect("the committed capture is a version 2 record");
    let workbook = format!("sha256:{}", sha256_hex(ORACLE_WORKBOOK));
    if capture.source != workbook {
        note!(
            "ACCEPT-NOT-RUN: {case}: awaiting capture: the committed capture is of {}, and the oracle workbook as committed is {workbook} (cargo xtask capture --pending takes it)",
            capture.source
        );
        return;
    }
    let isolation: Vec<_> = session.failures_at(STAGE_ISOLATION).collect();
    assert!(
        isolation.is_empty(),
        "the committed capture records isolation failures: {isolation:?}"
    );
    let environment = capture
        .environment
        .as_ref()
        .expect("the capture records Excel's environment");
    let other = other(true);
    let comparison = excel_comparison(&capture, &[rows, &other], rows)
        .unwrap_or_else(|refusal| panic!("calc_verdict::judge refuses the capture: {refusal}"));
    assert!(
        comparison.problems.is_empty(),
        "{} problems on sheet {} of the verdict on Excel {}'s capture ({}):\n{}",
        comparison.problems.len(),
        rows.sheet,
        environment.target.excel_build(),
        comparison.report.summary(),
        comparison.problems.join("\n")
    );
    note!(
        "ACCEPT-NOTE: {case}: Excel comparison through calc_verdict::judge against Excel {}'s capture of the oracle workbook: sheet {}: {} (every row of cfunc-m1.2 matched, every other row not modeled with its owner); the whole workbook: {}",
        environment.target.excel_build(),
        rows.sheet,
        sheet_verdicts(&comparison.report, rows),
        comparison.report.summary()
    );
}

// ---------------------------------------------------------------------------------------------
// cfunc_m1_2__mixed_operands

/// The cells the mixed-operand rows read; A5 and A6 are blank. C2 holds a text of 32767
/// characters, the longest a cell holds, and C1 one character less. C3 and C4 hold two numbers
/// of 14 significant digits that differ in the last, C5 holds 0, and C6 and C7 hold the texts
/// 1e-400 and 1e-309.
fn mixed_operands_sheet() -> TestSheet {
    let (shorter, longest) = ("a".repeat(32_766), "a".repeat(32_767));
    TestSheet::new(&[
        ("A1", "text \"2\"", text_value("2")),
        ("A2", "TRUE", CalcValue::logical(true)),
        ("A3", "5", CalcValue::number(5.0)),
        ("A4", "text \"a\"", text_value("a")),
        ("A7", "10", CalcValue::number(10.0)),
        ("B1", "0.5", CalcValue::number(0.5)),
        ("B2", "-2", CalcValue::number(-2.0)),
        ("B3", "10", CalcValue::number(10.0)),
        ("C1", "text of 32766 \"a\" characters", text_value(&shorter)),
        ("C2", "text of 32767 \"a\" characters", text_value(&longest)),
        (
            "C3",
            "123456789012.34",
            CalcValue::number(123_456_789_012.34),
        ),
        (
            "C4",
            "123456789012.33",
            CalcValue::number(123_456_789_012.33),
        ),
        ("C5", "0", CalcValue::number(0.0)),
        ("C6", "text \"1e-400\"", text_value("1e-400")),
        ("C7", "text \"1e-309\"", text_value("1e-309")),
    ])
}

/// The rows of cfunc_m1_2__mixed_operands, run through calc_func::invoke on `s`, in the order the
/// oracle workbook's sheet `mixed_operands` holds their formulas.
fn mixed_operands_rows(s: &mut TestSheet, quiet: bool) -> Rows {
    const CASE: &str = "cfunc_m1_2__mixed_operands";
    let mut rows = Rows::new(CASE, "mixed_operands", quiet);
    let r = &mut rows;
    const ZERO: &str = "a zero result is +0";

    // Arithmetic operands are read through worksheet number coercion, whether written directly
    // or dereferenced from a cell: text that reads as a number and logicals count, a blank cell
    // is 0, and other text and empty text are #VALUE!.
    row!(r, s, PLUS, [txt("1"), num("1")], N(2.0));
    row!(r, s, PLUS, [cell(s, "A1"), num("1")], N(3.0));
    row!(r, s, PLUS, [logical(true), num("1")], N(2.0));
    row!(r, s, TIMES, [cell(s, "A2"), num("5")], N(5.0));
    row!(
        r,
        s,
        PLUS,
        [txt("a"), num("1")],
        E(WorksheetErrorCode::Value)
    );
    row!(
        r,
        s,
        PLUS,
        [txt(""), num("1")],
        E(WorksheetErrorCode::Value)
    );
    row!(r, s, PLUS, [cell(s, "A5"), num("1")], N(1.0));
    row!(
        r,
        s,
        PLUS,
        [txt("TRUE"), num("1")],
        E(WorksheetErrorCode::Value)
    );
    row!(r, s, TIMES, [txt(" 1.5 "), num("2")], N(3.0));
    row!(r, s, DIVIDE, [txt("1e3"), num("1")], N(1000.0));
    row!(r, s, PLUS, [txt("-0"), num("0")], N(0.0));
    row!(
        r,
        s,
        PLUS,
        [txt("123456789012345"), num("0")],
        N(123_456_789_012_345.0)
    );
    row!(r, s, MINUS, [num("5"), txt("3")], N(2.0));
    row!(r, s, PLUS, [txt("+5"), num("0")], N(5.0));
    row!(r, s, PLUS, [txt(".5"), num("0")], N(0.5));
    row!(r, s, PLUS, [txt("5."), num("0")], N(5.0));
    row!(r, s, PLUS, [txt("1E+3"), num("0")], N(1000.0));
    // Decimal text is read as its nearest binary64; text beyond the range Excel reads is #VALUE!
    // (the rows at the end of the case measure that range's edges).
    row!(r, s, PLUS, [txt("0.1"), num("0")], N(0.1));
    row!(
        r,
        s,
        PLUS,
        [txt("1e-400"), num("0")],
        E(WorksheetErrorCode::Value)
    );
    // Text forms that need Excel's locale rules remain refused. Plain long decimal
    // text keeps 15 significant digits, as the unchanged D24/D25 captures measure.
    owned_by!(
        r,
        s,
        NUMBER_TEXT,
        PLUS,
        [txt("1/2"), num("0")],
        Refused(Some(0))
    );
    owned_by!(
        r,
        s,
        NUMBER_TEXT,
        PLUS,
        [txt("$5"), num("0")],
        Refused(Some(0))
    );
    owned_by!(
        r,
        s,
        NUMBER_TEXT,
        PLUS,
        [txt("1,000"), num("0")],
        Refused(Some(0))
    );
    owned_by!(
        r,
        s,
        NUMBER_TEXT,
        PLUS,
        [txt("50%"), num("0")],
        Refused(Some(0))
    );
    row!(
        r,
        s,
        PLUS,
        [txt("1234567890123456"), num("0")],
        N(1_234_567_890_123_450.0)
    );
    row!(
        r,
        s,
        PLUS,
        [txt("1.2345678901234567"), num("0")],
        N(1.23456789012345)
    );
    row!(
        r,
        s,
        PLUS,
        [num("0"), txt("1e400")],
        E(WorksheetErrorCode::Value)
    );
    // Results: an overflow is #NUM!, division by 0 is #DIV/0!, a zero is +0 whatever its sign,
    // and a subnormal result is refused. The literals are ones Excel loads from a file unchanged
    // (see the generator): 9.99999999999999E+307 is the largest number a cell can be given, and
    // 2.2250738585073E-308 is just above the smallest normal number.
    row!(
        r,
        s,
        TIMES,
        [num("9.99999999999999E+307"), num("10")],
        E(WorksheetErrorCode::Num)
    );
    row!(r, s, TIMES, [num("0"), num("-1")], N(0.0), ZERO);
    row!(
        r,
        s,
        DIVIDE,
        [num("1"), num("0")],
        E(WorksheetErrorCode::Div0)
    );
    row!(
        r,
        s,
        DIVIDE,
        [cell(s, "A5"), cell(s, "A6")],
        E(WorksheetErrorCode::Div0)
    );
    // A quotient below the normal range is +0, as Excel gives it (dc-b1l).
    row!(
        r,
        s,
        DIVIDE,
        [num("2.2250738585073E-308"), num("2")],
        N(0.0)
    );
    // ^ computes the powers that need a power algorithm by OxFunc's Excel power kernel (F1,
    // dc-8dq); Excel's capture of these rows is the check.
    row!(r, s, POWER, [txt("2"), num("2")], N(4.0));
    row!(r, s, POWER, [logical(true), num("2")], N(1.0));
    row!(r, s, POWER, [num("2"), num("3")], N(8.0));
    row!(r, s, POWER, [num("10"), num("-2")], N(0.01));
    row!(
        r,
        s,
        POWER,
        [num("2"), num("0.5")],
        N(std::f64::consts::SQRT_2)
    );
    row!(r, s, POWER, [num("3"), num("1")], N(3.0));
    // Written -(0): Excel loads the formula -0 from a file as the literal 0, with no negation.
    row!(
        r,
        s,
        NEGATE,
        [arg("(0)", CalcValue::number(0.0))],
        N(0.0),
        ZERO
    );
    row!(r, s, NEGATE, [txt("2")], N(-2.0));
    row!(r, s, NEGATE, [logical(true)], N(-1.0));
    row!(r, s, NEGATE, [cell(s, "A5")], N(0.0), ZERO);
    row!(r, s, NEGATE, [txt("")], E(WorksheetErrorCode::Value));
    row!(r, s, NEGATE, [num("2")], N(-2.0));
    row!(r, s, UNARY_PLUS, [txt("a")], t("a"));
    row!(r, s, UNARY_PLUS, [logical(true)], L(true));
    row!(r, s, UNARY_PLUS, [cell(s, "A5")], N(0.0));
    row!(r, s, PERCENT, [num("50")], N(0.5));
    row!(r, s, PERCENT, [txt("50")], N(0.5));
    row!(r, s, PERCENT, [logical(true)], N(0.01));
    // x% is x/100, one rounding: 35% and 57% give the nearest binary64 to 0.35 and 0.57, where
    // x*0.01 would give the next one up. 7% gives the same either way.
    row!(r, s, PERCENT, [num("7")], N(0.07));
    row!(r, s, PERCENT, [num("35")], N(0.35));
    row!(r, s, PERCENT, [num("57")], N(0.57));
    // A + or - whose result is near zero: calc-func gives the binary64 difference; whether Excel
    // gives 0 depends on the formula's final operation, which the caller knows.
    owned_by!(
        r,
        s,
        NEAR_ZERO,
        MINUS,
        [arg("0.5-0.4", CalcValue::number(0.5 - 0.4)), num("0.1")],
        N(-2.7755575615628914e-17)
    );
    owned_by!(
        r,
        s,
        NEAR_ZERO,
        TIMES,
        [
            num("1"),
            arg("(0.5-0.4-0.1)", CalcValue::number(0.5 - 0.4 - 0.1))
        ],
        N(-2.7755575615628914e-17)
    );
    owned_by!(
        r,
        s,
        NEAR_ZERO,
        MINUS,
        [
            arg(
                "1.333+1.225-1.333",
                CalcValue::number(1.333 + 1.225 - 1.333)
            ),
            num("1.225")
        ],
        N(-2.220446049250313e-16)
    );
    // Boundary rows for the measurement of that rule: a final - whose result is exact and not
    // zero (1E15+1-1E15 is 1), two residuals of decimal fractions, and the difference of two
    // nearly equal constant cells.
    owned_by!(
        r,
        s,
        NEAR_ZERO,
        MINUS,
        [arg("1E15+1", CalcValue::number(1e15 + 1.0)), num("1E15")],
        N(1.0)
    );
    owned_by!(
        r,
        s,
        NEAR_ZERO,
        MINUS,
        [arg("0.1+0.2", CalcValue::number(0.1 + 0.2)), num("0.3")],
        N(5.551115123125783e-17)
    );
    owned_by!(
        r,
        s,
        NEAR_ZERO,
        MINUS,
        [arg("1+1E-15", CalcValue::number(1.0 + 1e-15)), num("1")],
        N(1.1102230246251565e-15)
    );
    owned_by!(
        r,
        s,
        NEAR_ZERO,
        MINUS,
        [cell(s, "C3"), cell(s, "C4")],
        N(0.0099945068359375)
    );

    // Comparison coerces nothing: numbers order before text and text before logicals, a blank
    // compares as the other operand's empty value, and text compares ignoring case. Text ordering,
    // case-insensitive equality and nearly equal numbers follow Excel's collation and comparison
    // rule (cdoc-jpha's, measured by its own oracle workbook); these rows' values are those of this
    // workbook's capture.
    row!(r, s, EQ, [txt("1"), num("1")], L(false));
    row!(r, s, NE, [cell(s, "A1"), num("2")], L(true));
    row!(r, s, LT, [num("1"), txt("a")], L(true));
    row!(r, s, LT, [txt("a"), logical(true)], L(true));
    row!(
        r,
        s,
        GT,
        [logical(false), num("9.99999999999999E+307")],
        L(true)
    );
    row!(r, s, EQ, [txt("a"), txt("A")], L(true));
    row!(r, s, EQ, [cell(s, "A5"), num("0")], L(true));
    row!(r, s, EQ, [cell(s, "A5"), txt("")], L(true));
    row!(r, s, EQ, [cell(s, "A5"), logical(false)], L(true));
    row!(r, s, EQ, [cell(s, "A5"), cell(s, "A6")], L(true));
    row!(r, s, LT, [cell(s, "A5"), num("1")], L(true));
    row!(r, s, LT, [txt("abc"), txt("abd")], L(true));
    row!(r, s, GT, [txt("B"), txt("a")], L(true));
    row!(r, s, LT, [txt("a1"), txt("aa")], L(true));
    row!(r, s, LT, [txt("10"), txt("9")], L(true));
    row!(r, s, LT, [txt(""), txt("a")], L(true));
    row!(r, s, LE, [txt("a"), txt("A")], L(true));
    row!(r, s, GE, [num("1"), num("1")], L(true));
    row!(r, s, LE, [num("1"), num("0")], L(false));
    row!(r, s, GT, [logical(true), logical(false)], L(true));
    row!(r, s, EQ, [txt("\u{e9}"), txt("\u{e9}")], L(true));
    row!(r, s, EQ, [num("1"), num("2")], L(false));
    // Nearly equal numbers are equal when their 15-digit roundings are, and text compares with
    // Excel's collation, in which a hyphen weighs after the letters.
    row!(
        r,
        s,
        EQ,
        [arg("0.1+0.2", CalcValue::number(0.1 + 0.2)), num("0.3")],
        L(true)
    );
    row!(r, s, EQ, [txt("\u{e9}"), txt("\u{c9}")], L(true));
    row!(r, s, LT, [txt("a-b"), txt("ab")], L(false));
    row!(r, s, LT, [txt("apple pie"), txt("banana")], L(true));
    row!(r, s, LT, [txt("a"), txt("B")], L(true));
    row!(r, s, EQ, [txt("A"), txt("a")], L(true));
    row!(
        r,
        s,
        EQ,
        [arg("1+1E-15", CalcValue::number(1.0 + 1e-15)), num("1")],
        L(true)
    );

    // Concatenation: logicals as TRUE and FALSE, a blank as empty text, a number as its text
    // (dc-q9rb's dc_q9rb__number_text_for_concatenation measures that text).
    row!(r, s, AMPERSAND, [txt("a"), txt("b")], t("ab"));
    row!(r, s, AMPERSAND, [txt("a"), logical(true)], t("aTRUE"));
    row!(r, s, AMPERSAND, [txt("a"), logical(false)], t("aFALSE"));
    row!(r, s, AMPERSAND, [cell(s, "A5"), txt("x")], t("x"));
    row!(r, s, AMPERSAND, [cell(s, "A2"), cell(s, "A4")], t("TRUEa"));
    // dc_q9rb__number_text_for_concatenation measures these two formulas in Excel (its oracle
    // rows number_text_for_concatenation!D62 and D63).
    row!(r, s, AMPERSAND, [txt("a"), num("1")], t("a1"));
    row!(r, s, AMPERSAND, [num("1"), txt("a")], t("1a"));
    // A joined text of 32767 characters is a cell's longest; a longer one is cut to 32767.
    let (shorter, longest) = ("a".repeat(32_766), "a".repeat(32_767));
    row!(
        r,
        s,
        AMPERSAND,
        [cell(s, "C1"), txt("b")],
        t(&format!("{shorter}b"))
    );
    row!(r, s, AMPERSAND, [cell(s, "C2"), txt("b")], t(&longest));

    // SUM: direct text and logicals are coerced; the same values held by referenced cells or by
    // an array are skipped.
    row!(r, s, SUM, [txt("2"), logical(true)], N(3.0));
    row!(r, s, SUM, [reference(s, &["A1:A3"])], N(5.0));
    row!(
        r,
        s,
        SUM,
        [reference(s, &["A1:A3"]), txt("2"), logical(true)],
        N(8.0)
    );
    let array = CalcArray::from_rows(vec![vec![
        CalcValue::number(1.0),
        text_value("2"),
        CalcValue::logical(true),
    ]])
    .expect("a 1-by-3 array");
    row!(
        r,
        s,
        SUM,
        [arg("{1,\"2\",TRUE}", CalcValue::array(array))],
        N(1.0)
    );
    row!(r, s, SUM, [txt("a")], E(WorksheetErrorCode::Value));
    row!(r, s, SUM, [reference(s, &["A4"])], N(0.0));
    row!(r, s, SUM, [reference(s, &["A5"])], N(0.0));
    row!(r, s, SUM, [txt("")], E(WorksheetErrorCode::Value));
    row!(r, s, SUM, [num("1"), omitted()], N(1.0));
    row!(
        r,
        s,
        SUM,
        [num("9.99999999999999E+307"), num("9.99999999999999E+307")],
        E(WorksheetErrorCode::Num)
    );
    row!(
        r,
        s,
        SUM,
        [
            num("9.99999999999999E+307"),
            num("9.99999999999999E+307"),
            num("-9.99999999999999E+307")
        ],
        E(WorksheetErrorCode::Num),
        "a running total that overflows stays #NUM! when a later number would bring it back"
    );
    row!(r, s, SUM, [reference(s, &["B1:B3"])], N(8.5));
    row!(r, s, SUM, [reference(s, &["A3", "A7"])], N(15.0));
    s.reads.clear();
    s.visited = 0;
    row!(r, s, SUM, [reference(s, &["A:A"])], N(15.0));
    let column = s.area("A:A");
    assert_eq!(
        (s.reads.as_slice(), s.visited),
        (&[column][..], 5),
        "SUM(A:A) asks for the column once and visits only its 5 held cells"
    );
    if !quiet {
        note!(
            "ACCEPT-NOTE: {CASE}: SUM(A:A) asked the cell reader for {} area ({} cells) and was visited with {} held cells",
            s.reads.len(),
            column.cell_count(),
            s.visited
        );
    }

    // IF: the condition is read as a logical, and only the chosen branch is asked for.
    row!(r, s, IF, [txt("TRUE"), num("1"), num("2")], N(1.0));
    row!(r, s, IF, [txt("false"), num("1"), num("2")], N(2.0));
    row!(
        r,
        s,
        IF,
        [txt("x"), num("1"), num("2")],
        E(WorksheetErrorCode::Value)
    );
    row!(
        r,
        s,
        IF,
        [txt(""), num("1"), num("2")],
        E(WorksheetErrorCode::Value)
    );
    row!(
        r,
        s,
        IF,
        [txt("1"), num("1"), num("2")],
        E(WorksheetErrorCode::Value)
    );
    row!(r, s, IF, [cell(s, "A5"), num("1"), num("2")], N(2.0));
    row!(r, s, IF, [num("0"), num("1")], L(false));
    let asked = row!(
        r,
        s,
        IF,
        [
            num("2"),
            txt("a"),
            arg("1/0", CalcValue::error(WorksheetErrorCode::Div0))
        ],
        t("a")
    );
    assert_eq!(asked, [0, 1], "IF asked for its unselected branch");
    row!(r, s, IF, [logical(true), omitted(), num("2")], N(0.0));
    row!(r, s, IF, [logical(false), num("1"), omitted()], N(0.0));
    row!(r, s, IF, [omitted(), num("1"), num("2")], N(2.0));
    // A legacy formula (dc-m3u): Excel loads IF({TRUE,FALSE},1,2) from the file as
    // =@IF({TRUE,FALSE},1,2) and gives 1, the array's first element; calc_func refuses the array.
    let conditions = CalcArray::from_rows(vec![vec![
        CalcValue::logical(true),
        CalcValue::logical(false),
    ]])
    .expect("a 1-by-2 array");
    owned_by!(
        r,
        s,
        LEGACY_ARRAY_IF,
        IF,
        [
            arg("{TRUE,FALSE}", CalcValue::array(conditions)),
            num("1"),
            num("2")
        ],
        Refused(Some(0))
    );

    // Number text at the edges of the range Excel reads: its scale, the k for which its value is
    // 0.d1d2... times 10^k, must be from -308 to 308, whatever its sign, and text in binary64's
    // subnormal range reads as 0, which the products show apart from the + rows.
    use WorksheetErrorCode::Value;
    const ZERO_SCALE: &str = "a zero mantissa's digits after its point lower its scale";
    const SUBNORMAL_TEXT: &str = "text in binary64's subnormal range reads as 0";
    row!(r, s, PLUS, [txt("1e308"), num("0")], E(Value));
    row!(
        r,
        s,
        PLUS,
        [txt("9.99999999999999e307"), num("0")],
        N(9.99999999999999e307)
    );
    row!(r, s, PLUS, [txt("0e308"), num("0")], N(0.0));
    row!(r, s, PLUS, [txt("0e309"), num("0")], E(Value));
    row!(r, s, PLUS, [txt("0e-308"), num("0")], N(0.0));
    row!(r, s, PLUS, [txt("0e-309"), num("0")], E(Value));
    row!(r, s, PLUS, [txt("0.0e309"), num("0")], N(0.0), ZERO_SCALE);
    row!(
        r,
        s,
        PLUS,
        [txt("0.0e-308"), num("0")],
        E(Value),
        ZERO_SCALE
    );
    row!(
        r,
        s,
        PLUS,
        [txt("1e-309"), num("0")],
        N(0.0),
        SUBNORMAL_TEXT
    );
    row!(r, s, PLUS, [txt("9e-310"), num("0")], E(Value));
    row!(r, s, PLUS, [txt("0.01e309"), num("0")], N(1e307));
    row!(r, s, PLUS, [txt("1000e-310"), num("0")], N(1e-307));
    row!(r, s, PLUS, [txt("0.01e-308"), num("0")], E(Value));
    row!(
        r,
        s,
        TIMES,
        [txt("1e-309"), num("1E+300")],
        N(0.0),
        SUBNORMAL_TEXT
    );
    row!(
        r,
        s,
        TIMES,
        [txt("2.2250738585072E-308"), num("1E+300")],
        N(0.0),
        SUBNORMAL_TEXT
    );
    row!(r, s, PLUS, [txt("-1e308"), num("0")], E(Value));
    row!(
        r,
        s,
        TIMES,
        [txt("-1e-309"), num("1E+300")],
        N(0.0),
        SUBNORMAL_TEXT
    );

    // & cuts its result at 32767 UTF-16 code units, even between the halves of a surrogate pair:
    // C1 holds 32766 characters, and U+1F600 is two code units.
    let mut parted: Vec<u16> = "a".repeat(32_766).encode_utf16().collect();
    parted.push(0xD83D);
    row!(
        r,
        s,
        AMPERSAND,
        [cell(s, "C1"), txt("\u{1F600}")],
        Expect::Units(parted),
        "the cut parts a surrogate pair"
    );

    // Number text beyond the range Excel reads, and in binary64's subnormal range, read by COUNT
    // (cfunc-m3.4's function) and by COUNTIF (cfunc-m3.5's), in a criterion and in a cell of the
    // range; their own oracle workbooks hold no such row. calc-func computes each through
    // cfunc-m1.2's coercion: text beyond the range is no number, so COUNT does not count it, a
    // criterion holding it is text, which no number meets, and a range cell holding it meets no
    // number; text in the subnormal range reads as 0. Excel 16.0.20430.20182's capture of the
    // oracle workbook gives each of these six rows the value calc-func gives (0, 0, 0, 0, 1 and 1),
    // which the comparison does not score: each is published as not modeled with its owner.
    owned_by!(r, s, COUNT_TEXT, COUNT, [txt("1e-400")], N(0.0));
    owned_by!(
        r,
        s,
        CRITERIA_TEXT,
        COUNTIF,
        [reference(s, &["C5"]), txt("1e-400")],
        N(0.0)
    );
    owned_by!(
        r,
        s,
        CRITERIA_TEXT,
        COUNTIF,
        [reference(s, &["C5"]), txt("<1e308")],
        N(0.0)
    );
    owned_by!(
        r,
        s,
        CRITERIA_TEXT,
        COUNTIF,
        [reference(s, &["C6"]), num("0")],
        N(0.0)
    );
    owned_by!(r, s, COUNT_TEXT, COUNT, [txt("1e-309")], N(1.0));
    owned_by!(
        r,
        s,
        CRITERIA_TEXT,
        COUNTIF,
        [reference(s, &["C7"]), txt("1e-309")],
        N(1.0)
    );
    rows
}

/// The rows of cfunc_m1_2__mixed_operands on a fresh test sheet.
fn mixed_operands_run(quiet: bool) -> Rows {
    mixed_operands_rows(&mut mixed_operands_sheet(), quiet)
}

#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test)]
fn cfunc_m1_2__mixed_operands() {
    const CASE: &str = "cfunc_m1_2__mixed_operands";
    let fast = NumericMode::Fast;
    let mut sheet = mixed_operands_sheet();
    note!(
        "ACCEPT-NOTE: {CASE}: the sheet holds {}; A5 and A6 are blank",
        sheet.described()
    );
    let rows = mixed_operands_rows(&mut sheet, false);
    let s = &mut sheet;

    // Through the call interface alone (no Excel formula writes these): IF returns its chosen
    // reference unchanged, a call admits Missing and Empty and keeps both apart from empty text,
    // and a reference the cell reader refuses is a fault, not a value.
    let range = reference(s, &["A1:A3"]).value;
    let mut chosen = Recorded::new(vec![CalcValue::logical(true), range.clone()]);
    assert_eq!(run(surface(IF.0, IF.1), &mut chosen, &fast), Ok(range));
    for (target, operand, expected) in [
        (PLUS, CalcValue::missing(), Ok(1.0)),
        (PLUS, CalcValue::empty(), Ok(1.0)),
        (PLUS, text_value(""), Err(WorksheetErrorCode::Value)),
        (SUM, CalcValue::missing(), Ok(1.0)),
        (SUM, CalcValue::empty(), Ok(1.0)),
        (SUM, text_value(""), Err(WorksheetErrorCode::Value)),
    ] {
        let mut given = Recorded::new(vec![operand.clone(), CalcValue::number(1.0)]);
        let result = run(surface(target.0, target.1), &mut given, &fast);
        let observed = result
            .as_ref()
            .map(|value| value.as_number().ok_or(value.as_error()));
        assert_eq!(
            observed,
            Ok(expected.map_err(Some)),
            "{target:?} on {operand:?}: {result:?}"
        );
    }
    s.refuse = true;
    let refused_read = reference(s, &["A1:A3"]);
    let mut over_refusal = Recorded::new(vec![CalcValue::number(1.0), refused_read.value]);
    let sum = surface(SUM.0, SUM.1);
    assert_eq!(
        invoke(
            sum,
            &mut over_refusal,
            &mut EvalServices::with_cells(&fast, s),
            &mut FunctionCallScratch::new(),
        ),
        Err(FunctionFault::ReferenceUnreadable {
            id: sum.id(),
            index: 1
        })
    );
    s.refuse = false;

    // Unary plus publishes a number by the number rule: -0 gives +0, and a number that is no
    // worksheet value (NaN, an infinity) or is subnormal is refused, naming the operand.
    let unary_plus = surface(UNARY_PLUS.0, UNARY_PLUS.1);
    let mut negative_zero = Recorded::new(vec![CalcValue::number(-0.0)]);
    let published = run(unary_plus, &mut negative_zero, &fast);
    assert_eq!(
        published
            .as_ref()
            .map(|value| value.as_number().map(f64::to_bits)),
        Ok(Some(0x0000_0000_0000_0000)),
        "+(-0.0) gives {published:?}"
    );
    for operand in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY, 5e-324] {
        let mut given = Recorded::new(vec![CalcValue::number(operand)]);
        assert_eq!(
            run(unary_plus, &mut given, &fast),
            Err(FunctionFault::NotImplemented {
                id: unary_plus.id(),
                argument: Some(0)
            }),
            "+({operand:?})"
        );
    }
    note!(
        "ACCEPT-NOTE: {CASE}: +(-0.0) through calc_func::invoke gives bits {:#018x}; +NaN, +inf, +(-inf) and +5e-324 are refused naming the operand",
        published
            .as_ref()
            .ok()
            .and_then(CalcValue::as_number)
            .map_or(u64::MAX, f64::to_bits)
    );

    // Values that no Excel formula writes, each refused naming its argument: an IF condition that
    // is NaN, an infinity or subnormal (IF then asks for no branch), and an element of an array
    // given to SUM that is itself an array or a reference.
    let if_call = surface(IF.0, IF.1);
    for condition in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY, 5e-324] {
        let mut given = Recorded::new(vec![
            CalcValue::number(condition),
            CalcValue::number(1.0),
            CalcValue::number(2.0),
        ]);
        assert_eq!(
            run(if_call, &mut given, &fast),
            Err(FunctionFault::NotImplemented {
                id: if_call.id(),
                argument: Some(0)
            }),
            "IF({condition:?},1,2)"
        );
        assert_eq!(given.asked, [0], "IF({condition:?},1,2) asked for a branch");
    }
    // An array held in an array is read through (dc-h9u, Excel 16.0.20527: SUM of a GROUPBY
    // result whose LAMBDA returns arrays adds their elements); a reference held in an array is
    // refused.
    let nested = CalcValue::array(CalcArray::from_scalar(CalcValue::number(1.0)));
    let held =
        CalcArray::from_rows(vec![vec![CalcValue::number(1.0), nested]]).expect("a 1-by-2 array");
    let mut given = Recorded::new(vec![CalcValue::number(1.0), CalcValue::array(held)]);
    assert_eq!(
        run(sum, &mut given, &fast),
        Ok(CalcValue::number(3.0)),
        "SUM(1, an array holding 1 and an array holding 1)"
    );
    // A reference held in an array is read at its first cell (dc-b1l: SUM of OFFSET lifted over
    // an array); without a cell reader the call cannot supply it.
    let held = CalcArray::from_rows(vec![vec![CalcValue::number(1.0), a1_reference()]])
        .expect("a 1-by-2 array");
    let mut given = Recorded::new(vec![CalcValue::number(1.0), CalcValue::array(held)]);
    assert_eq!(
        run(sum, &mut given, &fast),
        Err(FunctionFault::ReferenceUnreadable {
            id: sum.id(),
            index: 1
        }),
        "SUM(1, an array holding 1 and a reference)"
    );
    note!(
        "ACCEPT-NOTE: {CASE}: IF with a NaN, +inf, -inf or 5e-324 condition is refused naming the condition and asks for no branch; SUM(1, an array holding 1 and a nested array) is 3 (dc-h9u) and SUM(1, an array holding 1 and a reference) reads the reference's first cell (dc-b1l), unreadable without a cell reader"
    );

    // NumericMode reaches every arithmetic site. Under Exact each call that rounds is refused:
    // the operators and SUM on numbers, reading text as a number, which rounds decimal text to
    // binary64, and comparing two different numbers, whose near-equality test rounds.
    let exact = exact_mode();
    let mut exact_refused = 0;
    let (mut refused_reading_text, mut refused_comparing) = (0, 0);
    for (target, args) in [
        (PLUS, vec![num("1"), num("2")]),
        (MINUS, vec![num("1"), num("2")]),
        (TIMES, vec![num("1"), num("2")]),
        (DIVIDE, vec![num("1"), num("2")]),
        (POWER, vec![num("3"), num("2")]),
        (POWER, vec![num("3"), num("-1")]),
        (PERCENT, vec![num("50")]),
        (SUM, vec![num("1"), num("2")]),
        (NEGATE, vec![txt("0.1")]),
        (POWER, vec![txt("0.1"), num("1")]),
        (SUM, vec![txt("2")]),
        (EQ, vec![num("1"), num("2")]),
        (LT, vec![cell(s, "A5"), num("1")]),
    ] {
        // Why the call rounds, counted from the call itself: a comparison operator tells two
        // different numbers apart; any other call given a text literal reads it as a number.
        if [EQ.1, LT.1].contains(&target.1) {
            refused_comparing += 1;
        } else if args.iter().any(|a| a.excel.starts_with('"')) {
            refused_reading_text += 1;
        }
        let resolved = surface(target.0, target.1);
        let mut given = Recorded::new(args.iter().map(|a| a.value.clone()).collect());
        let texts: Vec<&str> = args.iter().map(|a| a.excel.as_str()).collect();
        assert_eq!(
            run(resolved, &mut given, &exact),
            Err(FunctionFault::ExactUnavailable { id: resolved.id() }),
            "{:?} under Exact",
            resolved.oracle_formula(&texts)
        );
        exact_refused += 1;
    }
    // A call without rounding computes under Exact, to the value its Fast oracle row computed.
    let mut exact_computed = 0;
    for (target, args) in [
        (POWER, vec![num("3"), num("1")]),
        (NEGATE, vec![num("2")]),
        (NEGATE, vec![logical(true)]),
        (UNARY_PLUS, vec![txt("a")]),
        (GE, vec![num("1"), num("1")]),
        (LT, vec![num("1"), txt("a")]),
        (AMPERSAND, vec![txt("a"), txt("b")]),
        (DIVIDE, vec![num("1"), num("0")]),
        (SUM, vec![reference(s, &["A4"])]),
    ] {
        let resolved = surface(target.0, target.1);
        let texts: Vec<&str> = args.iter().map(|a| a.excel.as_str()).collect();
        let formula = resolved.oracle_formula(&texts).expect("a formula");
        let under_fast = rows
            .oracle
            .iter()
            .find(|row| row.formula == formula)
            .and_then(|row| row.outcome.as_ref().ok())
            .unwrap_or_else(|| panic!("no oracle row computes {formula}"));
        let mut given = Recorded::new(args.iter().map(|a| a.value.clone()).collect());
        let under_exact = invoke(
            resolved,
            &mut given,
            &mut EvalServices::with_cells(&exact, s),
            &mut FunctionCallScratch::new(),
        );
        assert!(
            under_exact
                .as_ref()
                .is_ok_and(|value| identical(value, under_fast)),
            "{formula} under Exact gives {under_exact:?}, under Fast {under_fast:?}"
        );
        exact_computed += 1;
    }
    // One comparison whose result differs between the modes: =1=2 is FALSE under Fast and
    // refused under Exact, because telling 1 from 2 runs the near-equality test.
    let equal = surface(EQ.0, EQ.1);
    let one_equals_two = |mode: &NumericMode| {
        let mut given = Recorded::new(vec![CalcValue::number(1.0), CalcValue::number(2.0)]);
        run(equal, &mut given, mode)
    };
    assert_eq!(
        (one_equals_two(&fast), one_equals_two(&exact)),
        (
            Ok(CalcValue::logical(false)),
            Err(FunctionFault::ExactUnavailable { id: equal.id() })
        )
    );
    note!(
        "ACCEPT-NOTE: {CASE}: under NumericMode::Exact {exact_refused} calls that round ({refused_reading_text} of them by reading text as a number, {refused_comparing} by comparing two different numbers) were refused with ExactUnavailable, and {exact_computed} calls without rounding computed the value of their Fast oracle row; =1=2 gives FALSE under Fast and ExactUnavailable under Exact"
    );

    // The retained Excel witness of the POWER function, applied to the ^ operator on the
    // assumption that ^ computes POWER: every result ^ computes equals the witness's bit for bit
    // or error for error, and since F1 (dc-8dq) every row computes.
    let power = surface(POWER.0, POWER.1);
    let witness = power_witness_rows();
    assert_eq!(witness.len(), 111, "the witness selection holds 111 rows");
    let (mut computed, mut errors, mut refused) = (0, 0, 0);
    for (id, x, y, expected) in &witness {
        let mut given = Recorded::new(vec![CalcValue::number(*x), CalcValue::number(*y)]);
        match run(power, &mut given, &fast) {
            Ok(value) => {
                let observed = match value.as_number() {
                    Some(n) => Ok(n.to_bits()),
                    None => Err(value.as_error().expect("a number or an error")),
                };
                assert_eq!(&observed, expected, "{id}: {x:?}^{y:?}");
                computed += 1;
                errors += usize::from(observed.is_err());
            }
            Err(FunctionFault::NotImplemented {
                id: refuser,
                argument: None,
            }) if refuser == power.id() => {
                refused += 1;
            }
            Err(fault) => panic!("{id}: {x:?}^{y:?}: {fault:?}"),
        }
    }
    assert_eq!(
        (computed, errors, refused),
        (111, 20, 0),
        "witness rows computed, of them errors, and refused (F1, dc-8dq: every row computes)"
    );
    // Excel's captured value of each oracle row of ^ that needs a power algorithm, against the witness's POWER
    // row for the same arguments where the witness holds one: equal bits support the assumption
    // for those arguments, and different bits would refute it.
    let mut captured_powers = Vec::new();
    if captured() {
        let capture = CaptureRecord::decode(ORACLE_CAPTURE).expect("the committed capture decodes");
        for (formula, x, y) in [
            ("=2^3", 2.0, 3.0),
            ("=10^-2", 10.0, -2.0),
            ("=2^0.5", 2.0, 0.5),
        ] {
            let row = rows
                .oracle
                .iter()
                .find(|row| row.formula == formula)
                .unwrap_or_else(|| panic!("no row {formula}"));
            let cell = oracle_cell(rows.sheet, row.row);
            let at = format!("{}!{}", cell.sheet(), cell.a1());
            let excel = capture
                .observations
                .iter()
                .find(|observed| observed.cell == cell)
                .and_then(|observed| match &observed.observation {
                    Observation::ExcelRaw(value) => value.as_number(),
                    _ => None,
                })
                .unwrap_or_else(|| panic!("the capture holds no raw number at {at}"));
            let witnessed = witness.iter().find(|(_, wx, wy, _)| *wx == x && *wy == y);
            if let Some((id, _, _, expected)) = witnessed {
                assert_eq!(
                    &Ok(excel.to_bits()),
                    expected,
                    "{at} {formula}: Excel's ^ against the witness's POWER row {id}"
                );
            }
            captured_powers.push(format!(
                "{at} {formula} {excel:?} (bits {:#018x}; {})",
                excel.to_bits(),
                witnessed.map_or_else(
                    || "the witness holds no row for these arguments".to_owned(),
                    |(id, ..)| format!("equal to the witness's POWER row {id}")
                )
            ));
        }
    }
    note!(
        "ACCEPT-NOTE: {CASE}: retained witness of the POWER function (migration/witnesses/w111-extended-power.json, {} rows), applied to ^ on the assumption that ^ computes POWER: ^ computed {computed} rows equal to Excel's POWER results ({errors} of them #NUM! or #DIV/0!) and refused {refused} rows that need a power algorithm; {}",
        witness.len(),
        if captured_powers.is_empty() {
            "the oracle's ^ rows that need a power algorithm are not captured".to_owned()
        } else {
            format!(
                "Excel's capture gives the oracle's ^ rows that need a power algorithm: {}",
                captured_powers.join(", ")
            )
        }
    );

    // The oracle workbook: this case's sheet holds exactly its rows and cells, and the committed
    // workbook is its generator's output.
    assert_oracle_sheet(&rows, s);
    #[cfg(all(windows, not(target_arch = "wasm32")))]
    {
        let generated = generated_fixtures();
        let names: Vec<&str> = generated.keys().map(String::as_str).collect();
        assert_eq!(
            names, GENERATED_WORKBOOKS,
            "the generator writes the oracle workbooks and nothing else"
        );
        let regenerated = &generated["cfunc_m1_2/oracle.xlsx"];
        assert!(
            regenerated.as_slice() == ORACLE_WORKBOOK,
            "the committed oracle workbook is not the generator's output (sha256 {} committed, {} generated)",
            sha256_hex(ORACLE_WORKBOOK),
            sha256_hex(regenerated)
        );
        note!(
            "ACCEPT-NOTE: {CASE}: crates/calc-func/tests/fixtures/make-fixtures.ps1, run now under Windows PowerShell, wrote the committed oracle workbook byte for byte ({} bytes)",
            regenerated.len()
        );
    }
    #[cfg(not(all(windows, not(target_arch = "wasm32"))))]
    {
        note!(
            "ACCEPT-NOTE: {CASE}: the generator runs under Windows PowerShell, so the check that the committed oracle workbook is its output runs on the windows route; here the workbook is checked against the rows only"
        );
    }

    rows_note(&rows);
    compare_with_excel(&rows, error_order_run);
}

/// The retained Excel witness of the POWER function: two arguments per row and the result Excel
/// returned, as binary64 bits or an error.
const POWER_WITNESS: &str = include_str!("../../../migration/witnesses/w111-extended-power.json");

type WitnessRow = (String, f64, f64, Result<u64, WorksheetErrorCode>);

/// The witness rows, one per line of the form
/// `{"id":"...","args":["0x...","0x..."],"expected_bits":"0x..." or "error:<code>"}`.
fn power_witness_rows() -> Vec<WitnessRow> {
    assert!(POWER_WITNESS.contains("\"function\": \"POWER\""));
    let bits = |hex: &str| {
        u64::from_str_radix(hex.strip_prefix("0x").expect("hex bits"), 16).expect("hex bits")
    };
    POWER_WITNESS
        .lines()
        .filter(|line| line.trim_start().starts_with("{\"id\":"))
        .map(|line| {
            let strings: Vec<&str> = line.split('"').skip(1).step_by(2).collect();
            assert_eq!(
                (strings.len(), strings[0], strings[2], strings[5]),
                (7, "id", "args", "expected_bits"),
                "{line}"
            );
            let expected = match strings[6] {
                "error:Num" => Err(WorksheetErrorCode::Num),
                "error:Div0" => Err(WorksheetErrorCode::Div0),
                "error:Value" => Err(WorksheetErrorCode::Value),
                hex => Ok(bits(hex)),
            };
            (
                strings[1].to_owned(),
                f64::from_bits(bits(strings[3])),
                f64::from_bits(bits(strings[4])),
                expected,
            )
        })
        .collect()
}

// ---------------------------------------------------------------------------------------------
// cfunc_m1_2__error_order

fn error_order_sheet() -> TestSheet {
    TestSheet::new(&[
        ("A1", "1", CalcValue::number(1.0)),
        ("B1", "#N/A", CalcValue::error(WorksheetErrorCode::NA)),
        ("A2", "#DIV/0!", CalcValue::error(WorksheetErrorCode::Div0)),
        ("B2", "2", CalcValue::number(2.0)),
    ])
}

/// The rows of cfunc_m1_2__error_order, run through calc_func::invoke on `s`, in the order the
/// oracle workbook's sheet `error_order` holds their formulas.
fn error_order_rows(s: &mut TestSheet, quiet: bool) -> Rows {
    let mut rows = Rows::new("cfunc_m1_2__error_order", "error_order", quiet);
    let r = &mut rows;
    use WorksheetErrorCode::{Div0, NA, Null, Ref, Value};
    const LEFT_FIRST: &str = "the left operand's error before the right operand's";
    const ARGUMENT_ORDER: &str = "the first direct argument's error, in argument order";
    const DIRECT_FIRST: &str =
        "a direct argument's error, a coercion's included, before a reference's or an array's";
    const HELD_ORDER: &str = "a reference's and an array's errors in argument order";
    const CHOSEN_BRANCH: &str = "only the chosen branch is read";

    // Two operands: an operand's error is the result. An arithmetic operator reads its operands
    // left to right, so a left operand that is not a number is #VALUE! before a right error; a
    // comparison and & read no operand as a number.
    let arithmetic = [PLUS, MINUS, TIMES, DIVIDE, POWER];
    let others = [EQ, NE, LT, LE, GT, GE, AMPERSAND];
    for operator in arithmetic.into_iter().chain(others) {
        row!(r, s, operator, [err(NA), err(Div0)], E(NA), LEFT_FIRST);
        row!(r, s, operator, [err(Div0), err(NA)], E(Div0), LEFT_FIRST);
        row!(r, s, operator, [num("1"), err(Ref)], E(Ref));
        if arithmetic.contains(&operator) {
            row!(
                r,
                s,
                operator,
                [err(NA), txt("a")],
                E(NA),
                "the left operand's error before the right operand is read as a number"
            );
            row!(
                r,
                s,
                operator,
                [txt("a"), err(NA)],
                E(Value),
                "the left operand's #VALUE! before the right operand's error"
            );
        } else {
            row!(r, s, operator, [err(NA), txt("a")], E(NA));
            row!(r, s, operator, [txt("a"), err(NA)], E(NA));
        }
    }
    row!(
        r,
        s,
        DIVIDE,
        [err(NA), num("0")],
        E(NA),
        "an operand's error before division by 0"
    );
    row!(
        r,
        s,
        DIVIDE,
        [txt("a"), num("0")],
        E(Value),
        "the left operand's #VALUE! before division by 0"
    );
    row!(
        r,
        s,
        DIVIDE,
        [num("0"), txt("a")],
        E(Value),
        "the right operand's #VALUE! before division by 0"
    );
    row!(
        r,
        s,
        POWER,
        [num("0"), err(NA)],
        E(NA),
        "an operand's error before the rules for a power of 0"
    );
    // No number's text is an error, so & returns the other operand's error.
    row!(r, s, AMPERSAND, [num("1"), err(NA)], E(NA));
    row!(r, s, NEGATE, [err(NA)], E(NA));
    row!(r, s, PERCENT, [err(Div0)], E(Div0));
    row!(r, s, UNARY_PLUS, [err(Ref)], E(Ref));

    // SUM: the first error among the direct arguments, in argument order, a coercion's #VALUE!
    // included; then the first error of the references and arrays in argument order, area by area
    // in a reference's order and row-major within an area. The cell reader is visited no further.
    row!(r, s, SUM, [err(NA), err(Div0)], E(NA), ARGUMENT_ORDER);
    row!(
        r,
        s,
        SUM,
        [num("1"), txt("a"), err(NA)],
        E(Value),
        ARGUMENT_ORDER
    );
    s.reads.clear();
    s.visited = 0;
    row!(
        r,
        s,
        SUM,
        [reference(s, &["A1:B2"])],
        E(NA),
        "row-major order within an area: B1 before A2"
    );
    assert_eq!(
        s.visited, 2,
        "the reader visited past B1, the first error in row-major order"
    );
    s.reads.clear();
    row!(
        r,
        s,
        SUM,
        [reference(s, &["A2:B2", "A1:B1"])],
        E(Div0),
        "the areas of a multi-area reference in its order"
    );
    assert_eq!(
        s.reads,
        [s.area("A2:B2")],
        "SUM read past the area holding its first error"
    );
    s.reads.clear();
    row!(
        r,
        s,
        SUM,
        [reference(s, &["A1:B2"]), err(Null)],
        E(Null),
        DIRECT_FIRST
    );
    assert!(
        s.reads.is_empty(),
        "SUM read a reference before its direct argument's error"
    );
    row!(
        r,
        s,
        SUM,
        [err(Ref), reference(s, &["A1:B2"])],
        E(Ref),
        DIRECT_FIRST
    );
    assert!(s.reads.is_empty(), "SUM read a reference after an error");
    row!(
        r,
        s,
        SUM,
        [
            num("9.99999999999999E+307"),
            num("9.99999999999999E+307"),
            err(NA)
        ],
        E(NA),
        "a direct argument's error before the fold, so before the running total overflows"
    );

    // IF: an error condition is the result before any branch is asked for; otherwise only the
    // chosen branch is asked for, and its error is the result.
    let asked = row!(
        r,
        s,
        IF,
        [err(NA), err(Div0), err(Ref)],
        E(NA),
        "the condition's error before any branch"
    );
    assert_eq!(asked, [0]);
    let asked = row!(
        r,
        s,
        IF,
        [logical(true), err(Div0), err(NA)],
        E(Div0),
        CHOSEN_BRANCH
    );
    assert_eq!(asked, [0, 1]);
    let asked = row!(
        r,
        s,
        IF,
        [logical(false), err(Div0), err(NA)],
        E(NA),
        CHOSEN_BRANCH
    );
    assert_eq!(asked, [0, 2]);
    let asked = row!(
        r,
        s,
        IF,
        [txt("x"), err(Div0), err(NA)],
        E(Value),
        "the condition's #VALUE! before any branch"
    );
    assert_eq!(asked, [0]);

    // SUM again: an array's error against a direct argument's error or #VALUE!, and against a
    // reference's error, each way round; a direct #VALUE! against a reference's error; a
    // reference's error after the running total overflowed.
    let held_ref = || {
        let array = CalcArray::from_rows(vec![vec![CalcValue::number(1.0), CalcValue::error(Ref)]])
            .expect("a 1-by-2 array");
        arg("{1,#REF!}", CalcValue::array(array))
    };
    row!(r, s, SUM, [held_ref(), err(NA)], E(NA), DIRECT_FIRST);
    row!(r, s, SUM, [err(NA), held_ref()], E(NA), DIRECT_FIRST);
    row!(r, s, SUM, [held_ref(), txt("a")], E(Value), DIRECT_FIRST);
    s.reads.clear();
    row!(
        r,
        s,
        SUM,
        [held_ref(), reference(s, &["A1:B2"])],
        E(Ref),
        HELD_ORDER
    );
    assert!(
        s.reads.is_empty(),
        "SUM read a reference after an earlier array's error"
    );
    row!(
        r,
        s,
        SUM,
        [reference(s, &["A1:B2"]), held_ref()],
        E(NA),
        HELD_ORDER
    );
    s.reads.clear();
    row!(
        r,
        s,
        SUM,
        [reference(s, &["B1"]), txt("a")],
        E(Value),
        DIRECT_FIRST
    );
    row!(
        r,
        s,
        SUM,
        [reference(s, &["A1:B2"]), txt("a")],
        E(Value),
        DIRECT_FIRST
    );
    assert!(
        s.reads.is_empty(),
        "SUM read a reference before its direct argument's #VALUE!"
    );
    row!(
        r,
        s,
        SUM,
        [
            num("9.99999999999999E+307"),
            num("9.99999999999999E+307"),
            reference(s, &["B1"])
        ],
        E(NA),
        "a reference's error after the running total overflowed"
    );
    rows
}

/// The rows of cfunc_m1_2__error_order on a fresh test sheet.
fn error_order_run(quiet: bool) -> Rows {
    error_order_rows(&mut error_order_sheet(), quiet)
}

#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test)]
fn cfunc_m1_2__error_order() {
    const CASE: &str = "cfunc_m1_2__error_order";
    let mut sheet = error_order_sheet();
    note!("ACCEPT-NOTE: {CASE}: the sheet holds {}", sheet.described());
    let rows = error_order_rows(&mut sheet, false);
    assert_oracle_sheet(&rows, &sheet);
    rows_note(&rows);
    compare_with_excel(&rows, mixed_operands_run);
}

// ---------------------------------------------------------------------------------------------
// cfunc_m1_2__excel_comparison: the Excel comparison, run on capture records built here in memory
// (never committed as a capture), so it carries no Excel evidence. It shows what the comparison
// of the two cases does with a capture: which verdicts it accepts and which it reports, and that
// it refuses a capture whenever calc_verdict::judge refuses it.

/// A capture record of the oracle workbook for `run`: a version 2 live capture of the workbook as
/// authored, under the calculation context it declares, by the committed worker, listing every
/// row's formula cell, and for each the raw value `observe` gives it (no observation when it gives
/// none).
fn capture_of(
    run: &[&Rows],
    observe: impl Fn(&str, &OracleRow) -> Option<CalcValue>,
) -> CaptureRecord {
    capture_in(ORACLE_WORKBOOK, CAPTURE_CASE, run, observe)
}

/// [`capture_of`] for the oracle workbook `package`, recorded under `capture_case`.
fn capture_in(
    package: &[u8],
    capture_case: &str,
    run: &[&Rows],
    observe: impl Fn(&str, &OracleRow) -> Option<CalcValue>,
) -> CaptureRecord {
    let source = format!("sha256:{}", sha256_hex(package));
    let mut sheets = Vec::new();
    let mut formulas = Vec::new();
    let mut observations = Vec::new();
    for rows in run {
        sheets.push(SheetEntry {
            name: rows.sheet.to_owned(),
            visibility: SheetVisibility::Visible,
        });
        for row in &rows.oracle {
            formulas.push(FormulaCell {
                cell: oracle_cell(rows.sheet, row.row),
                kind: FormulaKind::Ordinary,
                formula: Some(row.formula.clone()),
                dynamic_array: false,
            });
            if let Some(value) = observe(rows.sheet, row) {
                observations.push(CellObservation {
                    cell: oracle_cell(rows.sheet, row.row),
                    observation: Observation::ExcelRaw(value),
                });
            }
        }
    }
    CaptureRecord {
        case: capture_case.to_owned(),
        source: source.clone(),
        acquisition: Acquisition::LiveCapture,
        environment: None,
        sheets,
        formulas,
        observations,
        session: Some(CaptureSession {
            revision: AUTHORED_REVISION,
            operations: Vec::new(),
            context: package_context(package).fingerprint(),
            tool: format!("worker sha256:{}", sha256_hex(CAPTURE_WORKER)),
            input_package: source,
            output_package: None,
            failures: Vec::new(),
            shared_ranges: Vec::new(),
            saved: Vec::new(),
            reopened: Vec::new(),
        }),
    }
}

/// An observation that agrees with every row of cfunc-m1.2: its own value. A row of another bead
/// is observed as a text that differs from anything calc-func gives, so its verdict shows that it
/// is never compared.
fn agreeing(_: &str, row: &OracleRow) -> Option<CalcValue> {
    match (row.owner, &row.outcome) {
        (None, Ok(value)) => Some(value.clone()),
        _ => Some(text_value("an observation never compared")),
    }
}

/// What a changed capture observes for a row instead of its own value.
type ObservedChange = fn(&CalcValue) -> Option<CalcValue>;

#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test)]
fn cfunc_m1_2__excel_comparison() {
    const CASE: &str = "cfunc_m1_2__excel_comparison";
    let mixed = mixed_operands_run(true);
    let order = error_order_run(true);
    let run = [&mixed, &order];
    let compared = |capture: &CaptureRecord, case: &Rows| {
        excel_comparison(capture, &run, case)
            .unwrap_or_else(|refusal| panic!("judge refused the capture: {refusal}"))
    };

    // A capture that agrees with every row of cfunc-m1.2: each is matched, and each row of another
    // bead is not modeled with its owner, although its observation differs from calc-func's
    // value. The denominator is every formula cell of the workbook.
    let capture = capture_of(&run, agreeing);
    for case in run {
        let comparison = compared(&capture, case);
        assert_eq!(comparison.problems, Vec::<String>::new(), "{}", case.sheet);
    }
    let report = compared(&capture, &mixed).report;
    let all: usize = run.iter().map(|rows| rows.oracle.len()).sum();
    let elsewhere: usize = run.iter().map(|rows| rows.elsewhere()).sum();
    let owned = all - elsewhere;
    let counts = &report.counts;
    assert_eq!(
        (
            counts.denominator,
            counts.calculated,
            counts.matched,
            counts.not_modeled,
            counts.mismatched + counts.not_run + counts.unproven + counts.stale,
            counts.evaluator_calls
        ),
        (
            all as u64,
            owned as u64,
            owned as u64,
            elsewhere as u64,
            0,
            all as u64
        ),
        "{}",
        report.summary()
    );
    let computed_elsewhere = mixed
        .oracle
        .iter()
        .filter(|row| row.owner.is_some() && row.outcome.is_ok())
        .count();
    assert!(
        computed_elsewhere > 0 && mixed.refused() > 0,
        "rows of other beads both computed and refused"
    );

    // One changed observation of a row of cfunc-m1.2 is exactly one problem, at that row's cell.
    let first = |kind: fn(&CalcValue) -> bool| {
        mixed
            .oracle
            .iter()
            .find(|row| row.owner.is_none() && row.outcome.as_ref().is_ok_and(kind))
            .expect("a row of cfunc-m1.2 of that kind")
            .row
    };
    let number = first(|value| value.as_number().is_some_and(|n| n != 0.0));
    let zero = first(|value| value.as_number() == Some(0.0));
    let error = first(|value| value.as_error().is_some());
    let text = first(|value| value.as_text().is_some());
    let logical = first(|value| value.as_logical().is_some());
    let changed: Vec<(u32, ObservedChange)> = vec![
        (number, |value: &CalcValue| {
            Some(CalcValue::number(f64::from_bits(
                value.as_number()?.to_bits() ^ 1,
            )))
        }),
        (zero, |_: &CalcValue| Some(CalcValue::number(-0.0))),
        (number, |value: &CalcValue| {
            Some(text_value(&value.as_number()?.to_string()))
        }),
        (error, |value: &CalcValue| {
            Some(CalcValue::error(match value.as_error()? {
                WorksheetErrorCode::NA => WorksheetErrorCode::Value,
                _ => WorksheetErrorCode::NA,
            }))
        }),
        (text, |value: &CalcValue| {
            let mut units = value.as_text()?.utf16_code_units().to_vec();
            units.push(u16::from(b'x'));
            Some(CalcValue::text(ExcelText::from_utf16_code_units(units)))
        }),
        (logical, |value: &CalcValue| {
            Some(CalcValue::logical(!value.as_logical()?))
        }),
        (logical, |_: &CalcValue| None),
    ];
    for (changed_row, change) in &changed {
        let capture = capture_of(&run, |sheet, row| {
            if sheet == mixed.sheet && row.row == *changed_row {
                change(row.outcome.as_ref().expect("a computed row"))
            } else {
                agreeing(sheet, row)
            }
        });
        let cell = format!(
            "{}!{}",
            mixed.sheet,
            encode_a1(*changed_row, FORMULA_COLUMN)
        );
        let problems = compared(&capture, &mixed).problems;
        assert!(
            problems.len() == 1 && problems[0].starts_with(&format!("{cell} ")),
            "a capture changed at {cell} gives {problems:?}"
        );
        assert_eq!(compared(&capture, &order).problems, Vec::<String>::new());
    }

    // A row of another bead that calc-func computes, observed with the very value calc-func gives,
    // is still not modeled: it is never matched.
    let same = mixed
        .oracle
        .iter()
        .find(|row| row.owner.is_some() && row.outcome.is_ok())
        .expect("a computed row of another bead");
    let capture = capture_of(&run, |sheet, row| {
        if sheet == mixed.sheet && row.row == same.row {
            row.outcome.as_ref().ok().cloned()
        } else {
            agreeing(sheet, row)
        }
    });
    let comparison = compared(&capture, &mixed);
    assert_eq!(comparison.problems, Vec::<String>::new());
    assert_eq!(
        (
            comparison.report.counts.matched,
            comparison.report.counts.not_modeled
        ),
        (owned as u64, elsewhere as u64)
    );

    // The inventory is the denominator: a formula cell that no row has, and a row whose cell the
    // inventory lacks, are each one problem.
    let beyond = u32::try_from(mixed.oracle.len() + 1).expect("a row number");
    let mut extra = capture_of(&run, agreeing);
    extra.formulas.push(FormulaCell {
        cell: oracle_cell(mixed.sheet, beyond),
        kind: FormulaKind::Ordinary,
        formula: None,
        dynamic_array: false,
    });
    let mut lacking = capture_of(&run, agreeing);
    let gone = lacking.formulas.remove(0);
    lacking
        .observations
        .retain(|observed| observed.cell != gone.cell);
    for (capture, what) in [
        (&extra, "an extra formula cell"),
        (&lacking, "a lacking cell"),
    ] {
        let problems = compared(capture, &mixed).problems;
        assert_eq!(problems.len(), 1, "{what}: {problems:?}");
    }

    // The proof of calculation is the record of invoke calls: a result whose call is not in it is
    // unproven, not matched.
    let (results, mut evaluations) = published(&run);
    let dropped = evaluations.evaluations.remove(0);
    let problems = judged(&capture_of(&run, agreeing), &results, &evaluations, &mixed)
        .expect("judge admits the capture")
        .problems;
    let cell = format!("{}!{}", dropped.cell.sheet(), dropped.cell.a1());
    assert!(
        problems.len() == 1
            && problems[0].starts_with(&format!("{cell} "))
            && problems[0].contains("unproven"),
        "a result without its call: {problems:?}"
    );

    // A capture taken under another calculation context leaves every row of cfunc-m1.2 stale.
    let mut iterating = capture_of(&run, agreeing);
    iterating.session.as_mut().expect("a session").context = CalculationContext {
        iteration: true,
        ..workbook_context()
    }
    .fingerprint();
    let problems = compared(&iterating, &mixed).problems;
    assert_eq!(
        problems.len(),
        mixed.oracle.len() - mixed.elsewhere(),
        "{problems:?}"
    );
    assert!(problems.iter().all(|problem| problem.contains("stale")));

    // A capture that failed to establish its inventory is refused, as judge refuses it, even when
    // the failure names no cell.
    let mut unlisted = capture_of(&run, agreeing);
    unlisted
        .session
        .as_mut()
        .expect("a session")
        .failures
        .push(CaptureFailure {
            stage: STAGE_INVENTORY.to_owned(),
            cell: None,
            message: "a sheet's cells were not read".to_owned(),
        });
    match excel_comparison(&unlisted, &run, &mixed) {
        Err(JudgeError::Inconsistent { message }) if message.contains("inventory failure") => {}
        Err(refusal) => panic!("refused for another reason: {refusal}"),
        Ok(comparison) => panic!(
            "a capture with an inventory failure was judged: {}",
            comparison.report.summary()
        ),
    }

    note!(
        "ACCEPT-NOTE: {CASE}: on in-memory captures of the oracle workbook ({all} rows: {owned} of cfunc-m1.2, {elsewhere} of other beads), calc_verdict::judge's report through the comparison: the agreeing capture matches every row of cfunc-m1.2 and counts every other row not modeled, a row of another bead observed with calc-func's own value included; each of {} single-cell changes (a number's last bit, +0 as -0, a number as text, an error code, a text, a logical, a missing observation) is exactly one problem at its cell; an extra and a lacking formula cell are one problem each; a result whose invoke call is not in the evaluation record is unproven; a capture under another calculation context leaves the {} rows of cfunc-m1.2 on the sheet stale; a capture with a cell-less inventory failure is refused",
        changed.len(),
        mixed.oracle.len() - mixed.elsewhere()
    );
}

// ---------------------------------------------------------------------------------------------
// cfunc-m3.4: the aggregate and counting functions (SUM, PRODUCT, SUMSQ, SUMPRODUCT, COUNT,
// COUNTA, COUNTBLANK, AVERAGE, AVERAGEA, MIN, MAX, MINA and MAXA) through calc_func::invoke on
// catalogue targets.
//
// Each oracle row runs one call and asserts a value derived here by hand, and says how. A
// `documented` row applies a rule that Microsoft's page for the function states, named in the
// row. An `unmeasured choice` row applies a rule that no page settles: the page is silent, it
// contradicts itself (text in an AVERAGEA, MINA or MAXA reference), or its text disagrees with
// the rule this implementation follows (logicals and number text typed into AVERAGE, an error in
// a SUMSQ reference); each such disagreement is listed as divergent in the per-ID manifest of
// `migration/ports/cfunc-m3.4.json`. A `refused` row is a call that ends in a fault, listed there
// as unsupported: the kernel refuses it as not modelled, or the call interface refuses its
// argument at admission. No row has been compared with Excel.
//
// `tests/fixtures/cfunc_m3_4/oracle.xlsx`, written by the committed generator
// `tests/fixtures/make-fixtures.ps1`, holds every row's formula in column D of a sheet named after
// its case, in the order the case runs its rows, beside the cells the rows read, so that the owner
// can capture Excel's values; each case checks that its sheet holds exactly those formulas and
// cells. The cases do not wait for that capture: green shows the rules as written here, not
// agreement with Excel.

/// The aggregate oracle workbook, as committed (its bytes are kept across checkouts by
/// `.gitattributes`).
const AGGREGATE_WORKBOOK: &[u8] = include_bytes!("fixtures/cfunc_m3_4/oracle.xlsx");
const AGGREGATE_WORKBOOK_PATH: &str = "crates/calc-func/tests/fixtures/cfunc_m3_4/oracle.xlsx";
/// The bead's migration record, whose per-ID manifest the argument-class case checks against the
/// catalogue.
const AGGREGATE_RECORD: &str = include_str!("../../../migration/ports/cfunc-m3.4.json");

/// The family's entries: each spelling and the member number of its FunctionId in family 4.
const AGGREGATES: [(&str, u32); 13] = [
    ("SUM", 1),
    ("PRODUCT", 2),
    ("SUMSQ", 3),
    ("SUMPRODUCT", 4),
    ("COUNT", 5),
    ("COUNTA", 6),
    ("COUNTBLANK", 7),
    ("AVERAGE", 8),
    ("AVERAGEA", 9),
    ("MIN", 10),
    ("MAX", 11),
    ("MINA", 12),
    ("MAXA", 13),
];

/// A named call's surface.
const fn named(spelling: &'static str) -> (Notation, &'static str) {
    (Notation::Call, spelling)
}

/// Why a cfunc-m3.4 row expects its value.
#[derive(Clone, Copy, Debug)]
enum Basis {
    /// Derived by hand from this rule, which the function's documentation states.
    Documented(&'static str),
    /// Derived by hand from this rule, which no documentation settles.
    Choice(&'static str),
    /// Derived by hand from this rule, which another bead measured in Excel and compares with its
    /// capture; the rule names that bead.
    Measured(&'static str),
    /// Refused, for this reason: as not modelled by the kernel, or at admission.
    Unsupported(&'static str),
}

use Basis::{Choice, Documented, Measured, Unsupported};

/// A row of cfunc-m3.4: [`basis_row`] for that bead.
fn aggregate_row(
    rows: &mut Rows,
    sheet: &mut TestSheet,
    surface: (Notation, &str),
    args: &[Arg],
    expect: Expect,
    basis: Basis,
) {
    basis_row("cfunc-m3.4", rows, sheet, surface, args, expect, basis);
}

/// Runs the named call `surface` on `args` through calc_func::invoke under Fast with the test
/// sheet as the cell reader and asserts `expect`, that the eager call asked for each argument once
/// in order, and that the result fits `basis` (a value, or a refusal for an unsupported row). Then
/// records the row, a row of `bead`, with the next cell of column D and prints it with its basis.
fn basis_row(
    bead: &str,
    rows: &mut Rows,
    sheet: &mut TestSheet,
    surface: (Notation, &str),
    args: &[Arg],
    expect: Expect,
    basis: Basis,
) {
    let (formula, asked, result, tick) =
        call_and_assert(sheet, &NumericMode::Fast, surface, args, &expect);
    let in_order: Vec<usize> = (0..args.len()).collect();
    assert_eq!(asked, in_order, "{formula}: the arguments asked for");
    match basis {
        Unsupported(_) => assert!(
            result.is_err(),
            "{formula}: an unsupported row computed {result:?}"
        ),
        Documented(_) | Choice(_) | Measured(_) => assert!(
            result.is_ok(),
            "{formula}: a row of {bead} computes a value"
        ),
    }
    if let Measured(_) = basis {
        rows.measured += 1;
    }
    let row = u32::try_from(rows.oracle.len() + 1).expect("a row number");
    if !rows.quiet {
        let reads: Vec<&str> = args
            .iter()
            .flat_map(|a| a.reads.iter().map(String::as_str))
            .collect();
        let cells = if reads.is_empty() {
            String::new()
        } else {
            format!(" with {}", reads.join(", "))
        };
        let at = format!("{}!{}", rows.sheet, encode_a1(row, FORMULA_COLUMN));
        let label = match basis {
            Documented(rule) => format!("hand-derived from the documented rule: {rule}"),
            Choice(rule) => format!("hand-derived; unmeasured choice: {rule}"),
            Measured(rule) => format!("hand-derived from a rule measured in Excel: {rule}"),
            Unsupported(reason) => match &result {
                Err(FunctionFault::NotImplemented { .. }) => {
                    format!("refused as not modelled: {reason}")
                }
                Err(fault) => format!("refused with {fault:?}: {reason}"),
                Ok(value) => unreachable!("an unsupported row computed {value:?}"),
            },
        };
        // A measured row's rule was measured in Excel, so the suffix is left off its note.
        let compared = match basis {
            Measured(_) => "",
            Documented(_) | Choice(_) | Unsupported(_) => "; not compared with Excel",
        };
        note!(
            "ACCEPT-NOTE: {}: oracle {at} {formula}{cells} -> {} ({label}{compared})",
            rows.case,
            expect.shown()
        );
    }
    rows.calls.push(Call { row, tick });
    rows.oracle.push(OracleRow {
        row,
        formula,
        outcome: result,
        choice: match basis {
            Choice(rule) => Some(rule),
            Documented(_) | Measured(_) | Unsupported(_) => None,
        },
        owner: None,
        tick,
    });
}

/// Runs a row of cfunc-m3.4: `agg!(rows, sheet, "SPELLING", [args], expect, basis)`.
macro_rules! agg {
    ($rows:expr, $sheet:expr, $spelling:expr, [$($arg:expr),* $(,)?], $expect:expr, $basis:expr) => {{
        let args = [$($arg),*];
        aggregate_row($rows, $sheet, named($spelling), &args, $expect, $basis)
    }};
}

/// An array constant, as a formula writes it and as the caller supplies it.
fn array_arg(excel: &str, rows: Vec<Vec<CalcValue>>) -> Arg {
    arg(
        excel,
        CalcValue::array(CalcArray::from_rows(rows).expect("a rectangular array")),
    )
}

/// A reader over the test sheet's cells that also visits every blank cell of an area, as Empty,
/// which the CellReader contract allows.
struct DenseReader<'s>(&'s TestSheet);

impl CellReader for DenseReader<'_> {
    fn subtotal_exclusions(
        &mut self,
        _: calc_func::value::GridArea,
    ) -> Option<calc_func::SubtotalExclusions> {
        None
    }

    fn read_area(
        &mut self,
        area: GridArea,
        visit: &mut dyn FnMut(CellKey, &CalcValue) -> ControlFlow<()>,
    ) -> Result<(), Unreadable> {
        let (first, last) = (area.first(), area.last());
        for row in first.row()..=last.row() {
            for column in first.column()..=last.column() {
                let key = CellKey::new(area.sheet(), row, column).expect("a cell of the area");
                let value = self
                    .0
                    .cells
                    .get(&(row, column))
                    .cloned()
                    .unwrap_or_else(CalcValue::empty);
                if visit(key, &value).is_break() {
                    return Ok(());
                }
            }
        }
        Ok(())
    }
}

/// How a reader breaks the CellReader contract.
#[derive(Clone, Copy, Debug)]
enum Breach {
    /// After the held cells of the area, it also visits this cell, which lies outside the area.
    Outside(&'static str),
    /// It visits the held cells of the area in reverse row-major order.
    Reversed,
    /// It visits every held cell of the area twice.
    Twice,
}

/// A reader over the test sheet's cells that breaks the CellReader contract by `breach`.
struct BreachingReader<'s> {
    sheet: &'s TestSheet,
    breach: Breach,
}

impl CellReader for BreachingReader<'_> {
    fn subtotal_exclusions(
        &mut self,
        _: calc_func::value::GridArea,
    ) -> Option<calc_func::SubtotalExclusions> {
        None
    }

    fn read_area(
        &mut self,
        area: GridArea,
        visit: &mut dyn FnMut(CellKey, &CalcValue) -> ControlFlow<()>,
    ) -> Result<(), Unreadable> {
        let sheet = self.sheet;
        let mut cells: Vec<(CellKey, CalcValue)> = sheet
            .cells
            .iter()
            .filter_map(|(&(row, column), value)| {
                let key = CellKey::new(sheet.id, row, column).expect("a held cell");
                area.contains(key).then(|| (key, value.clone()))
            })
            .collect();
        match self.breach {
            Breach::Outside(a1) => {
                let key = sheet.key(a1);
                assert!(!area.contains(key), "{a1} lies outside the area");
                cells.push((key, sheet.value(a1)));
            }
            Breach::Reversed => cells.reverse(),
            Breach::Twice => {
                let again = cells.clone();
                cells.extend(again);
            }
        }
        for (key, value) in &cells {
            if visit(*key, value).is_break() {
                break;
            }
        }
        Ok(())
    }
}

/// The value an oracle row of `rows` computed for `formula`.
fn row_value<'r>(rows: &'r Rows, formula: &str) -> &'r CalcValue {
    rows.oracle
        .iter()
        .find(|row| row.formula == formula)
        .and_then(|row| row.outcome.as_ref().ok())
        .unwrap_or_else(|| panic!("no oracle row computes {formula}"))
}

/// Prints how many rows a case ran and on what basis each expects its value.
fn aggregate_rows_note(rows: &Rows) {
    let (choices, refused) = (rows.choices(), rows.refused());
    note!(
        "ACCEPT-NOTE: {}: {} rows through calc_func::invoke, each asserted against a value or a refusal derived here by hand and each with its formula on sheet {} of {AGGREGATE_WORKBOOK_PATH}: {} from documented rules, {} unmeasured choices, {} refused; none is compared with Excel until the owner captures that workbook",
        rows.case,
        rows.oracle.len(),
        rows.sheet,
        rows.oracle.len() - choices - refused,
        choices,
        refused
    );
}

// Rules the rows apply, as the functions' pages state them.
const TYPED_COUNTED: &str = "logicals and number text typed as arguments are counted";
const UNREADABLE_TEXT: &str = "text that cannot be read as a number causes an error";
const ERRORS_ERROR: &str = "error values cause errors";
const OMITTED_ZERO: &str = "an omitted argument is the number 0";

// ---------------------------------------------------------------------------------------------
// cfunc_m3_4__argument_classes

/// The cells the argument-class rows read: A1:A8 hold one of each class (A5 is blank and A6 holds
/// empty text), B1:B3 hold an error between two numbers, and C1:C8 hold numbers.
fn argument_classes_sheet() -> TestSheet {
    let ten = || CalcValue::number(10.0);
    TestSheet::new(&[
        ("A1", "1", CalcValue::number(1.0)),
        ("A2", "text \"2\"", text_value("2")),
        ("A3", "TRUE", CalcValue::logical(true)),
        ("A4", "text \"a\"", text_value("a")),
        ("A6", "empty text", text_value("")),
        ("A7", "4", CalcValue::number(4.0)),
        ("A8", "FALSE", CalcValue::logical(false)),
        ("B1", "5", CalcValue::number(5.0)),
        ("B2", "#DIV/0!", CalcValue::error(WorksheetErrorCode::Div0)),
        ("B3", "-3", CalcValue::number(-3.0)),
        ("C1", "10", ten()),
        ("C2", "10", ten()),
        ("C3", "10", ten()),
        ("C4", "10", ten()),
        ("C5", "10", ten()),
        ("C6", "10", ten()),
        ("C7", "0.5", CalcValue::number(0.5)),
        ("C8", "10", ten()),
    ])
}

/// The rows of cfunc_m3_4__argument_classes, run through calc_func::invoke on `s`, in the order
/// the aggregate oracle workbook's sheet `argument_classes` holds their formulas.
fn argument_classes_rows(s: &mut TestSheet, quiet: bool) -> Rows {
    const CASE: &str = "cfunc_m3_4__argument_classes";
    let mut rows = Rows::new(CASE, "argument_classes", quiet);
    let r = &mut rows;
    use WorksheetErrorCode::{Div0, NA, Num, Ref, Value};
    let mixed = |s: &TestSheet| reference(s, &["A1:A8"]);
    let with_error = |s: &TestSheet| reference(s, &["B1:B3"]);
    let typed_array = || {
        array_arg(
            "{1,\"2\",TRUE}",
            vec![vec![
                CalcValue::number(1.0),
                text_value("2"),
                CalcValue::logical(true),
            ]],
        )
    };

    // SUM keeps cfunc-m1.2's rules and order; these rows add the reduction order and the
    // classes of A1:A8.
    agg!(
        r,
        s,
        "SUM",
        [mixed(s)],
        N(5.0),
        Choice(
            "only the numbers a reference holds are added: the SUM page states that its text is ignored, and cfunc-m1.2 skips its logicals and blank cells the same way"
        )
    );
    agg!(
        r,
        s,
        "SUM",
        [num("1E16"), num("1"), num("-1E16")],
        N(0.0),
        Choice(
            "the sequential left fold in reading order: 1E16+1 rounds to 1E16 before -1E16 is added (the reduction order dc-6qki is to document)"
        )
    );
    agg!(
        r,
        s,
        "SUM",
        [with_error(s)],
        E(Div0),
        Choice("an error a reference holds is the result; the SUM page is silent")
    );
    agg!(
        r,
        s,
        "SUM",
        [num("2"), logical(true)],
        N(3.0),
        Choice(
            "a logical typed as an argument is read as a number, TRUE as 1, where a logical a reference holds is skipped (row SUM(A1:A8)); as cfunc-m1.2's SUM reads it and the SUMSQ, MIN and MAX pages state for those functions"
        )
    );
    agg!(
        r,
        s,
        "SUM",
        [num("2"), txt("3")],
        N(5.0),
        Choice(
            "number text typed as an argument is read as a number, where number text a reference holds is skipped (row SUM(A1:A8)); as cfunc-m1.2's SUM reads it and the SUMSQ, MIN and MAX pages state for those functions"
        )
    );

    // PRODUCT.
    agg!(
        r,
        s,
        "PRODUCT",
        [num("2"), num("3"), num("4")],
        N(24.0),
        Documented("PRODUCT multiplies the numbers given as its arguments")
    );
    agg!(
        r,
        s,
        "PRODUCT",
        [mixed(s)],
        N(4.0),
        Documented(
            "only the numbers of a reference are multiplied; its empty cells, logicals and text are ignored"
        )
    );
    agg!(
        r,
        s,
        "PRODUCT",
        [array_arg(
            "{2,\"x\",TRUE}",
            vec![vec![
                CalcValue::number(2.0),
                text_value("x"),
                CalcValue::logical(true)
            ]]
        )],
        N(2.0),
        Documented(
            "only the numbers of an array are multiplied; its logicals and text are ignored"
        )
    );
    agg!(
        r,
        s,
        "PRODUCT",
        [txt("2"), logical(true)],
        N(2.0),
        Choice(
            "logicals and number text typed as arguments are read as numbers, as the SUMSQ, MIN and MAX pages state; the PRODUCT page is silent"
        )
    );
    agg!(
        r,
        s,
        "PRODUCT",
        [txt("a")],
        E(Value),
        Choice(
            "typed text that does not read as a number is #VALUE!, as the SUMSQ page states; the PRODUCT page is silent"
        )
    );
    agg!(
        r,
        s,
        "PRODUCT",
        [reference(s, &["A3:A6"])],
        N(0.0),
        Choice("no number to multiply gives 0; the page is silent")
    );
    agg!(
        r,
        s,
        "PRODUCT",
        [with_error(s)],
        E(Div0),
        Choice("an error a reference holds is the result; the page is silent")
    );
    agg!(
        r,
        s,
        "PRODUCT",
        [num("2"), omitted()],
        N(2.0),
        Measured(
            "dc-bd5: PRODUCT passes over an omitted argument; PRODUCT(2,) is 2 (Excel 16.0.20430, cfunc_m3_4's capture)"
        )
    );
    agg!(
        r,
        s,
        "PRODUCT",
        [num("1E-200"), num("1E-200")],
        N(0.0),
        Measured(
            "dc-bd5: a product below the normal range is 0 (Excel 16.0.20430, cfunc_m3_4's capture)"
        )
    );

    // SUMSQ.
    agg!(
        r,
        s,
        "SUMSQ",
        [num("2"), num("3"), num("4")],
        N(29.0),
        Documented("SUMSQ adds the squares of its numbers")
    );
    agg!(
        r,
        s,
        "SUMSQ",
        [txt("2"), logical(true)],
        N(5.0),
        Documented(TYPED_COUNTED)
    );
    agg!(
        r,
        s,
        "SUMSQ",
        [txt("a")],
        E(Value),
        Documented(UNREADABLE_TEXT)
    );
    agg!(
        r,
        s,
        "SUMSQ",
        [mixed(s)],
        N(17.0),
        Documented(
            "only the numbers of a reference count; its empty cells, logicals and text are ignored"
        )
    );
    agg!(
        r,
        s,
        "SUMSQ",
        [array_arg(
            "{3,TRUE}",
            vec![vec![CalcValue::number(3.0), CalcValue::logical(true)]]
        )],
        N(9.0),
        Documented("only the numbers of an array count")
    );
    agg!(
        r,
        s,
        "SUMSQ",
        [with_error(s)],
        E(Div0),
        Choice(
            "divergent from the SUMSQ page, which says an error value in a reference is ignored: this kernel returns it, as every other aggregate here does"
        )
    );
    agg!(
        r,
        s,
        "SUMSQ",
        [num("1E-200")],
        N(0.0),
        Measured(
            "dc-bd5: a square below the normal range is 0 (Excel 16.0.20430, cfunc_m3_4's capture)"
        )
    );

    // SUMPRODUCT.
    let one_two = || {
        array_arg(
            "{1,2}",
            vec![vec![CalcValue::number(1.0), CalcValue::number(2.0)]],
        )
    };
    let three_four = || {
        array_arg(
            "{3,4}",
            vec![vec![CalcValue::number(3.0), CalcValue::number(4.0)]],
        )
    };
    agg!(
        r,
        s,
        "SUMPRODUCT",
        [one_two(), three_four()],
        N(11.0),
        Documented("SUMPRODUCT multiplies corresponding entries and adds the products")
    );
    agg!(
        r,
        s,
        "SUMPRODUCT",
        [array_arg(
            "{1,4,9}",
            vec![vec![
                CalcValue::number(1.0),
                CalcValue::number(4.0),
                CalcValue::number(9.0)
            ]]
        )],
        N(14.0),
        Documented(
            "the products of corresponding entries of the arrays given, here one, are added"
        )
    );
    agg!(
        r,
        s,
        "SUMPRODUCT",
        [
            array_arg(
                "{1,TRUE}",
                vec![vec![CalcValue::number(1.0), CalcValue::logical(true)]]
            ),
            three_four()
        ],
        N(3.0),
        Documented("a non-numeric entry is treated as 0")
    );
    agg!(
        r,
        s,
        "SUMPRODUCT",
        [mixed(s), reference(s, &["C1:C8"])],
        N(12.0),
        Documented("a non-numeric entry is treated as 0")
    );
    agg!(
        r,
        s,
        "SUMPRODUCT",
        [reference(s, &["A7:A8"]), reference(s, &["C6:C7"])],
        N(40.0),
        Documented(
            "corresponding entries are those at one position of each array, here of areas that start on different rows: A7 times C6, plus A8, a logical and so 0, times C7"
        )
    );
    agg!(
        r,
        s,
        "SUMPRODUCT",
        [one_two(), num("3")],
        E(Value),
        Choice(
            "a number given directly is a 1-by-1 array, so its dimensions differ from {1,2}'s, and arrays of different dimensions are #VALUE! (documented)"
        )
    );
    agg!(
        r,
        s,
        "SUMPRODUCT",
        [with_error(s), reference(s, &["C1:C3"])],
        E(Div0),
        Choice("an error entry is the result; the page is silent")
    );
    agg!(
        r,
        s,
        "SUMPRODUCT",
        [
            one_two(),
            array_arg(
                "{1,2,3}",
                vec![vec![
                    CalcValue::number(1.0),
                    CalcValue::number(2.0),
                    CalcValue::number(3.0)
                ]]
            ),
            array_arg(
                "{1,#N/A}",
                vec![vec![CalcValue::number(1.0), CalcValue::error(NA)]]
            )
        ],
        E(NA),
        Choice(
            "every argument is read before the dimensions are compared, so an error entry of the third array comes before the #VALUE! of the first two"
        )
    );
    agg!(
        r,
        s,
        "SUMPRODUCT",
        [num("2"), num("3")],
        N(6.0),
        Choice("a number given directly is a 1-by-1 array")
    );
    agg!(
        r,
        s,
        "SUMPRODUCT",
        [txt("3")],
        N(0.0),
        Choice("text given directly, number text included, is a non-numeric 1-by-1 entry, so 0")
    );
    agg!(
        r,
        s,
        "SUMPRODUCT",
        [reference(s, &["A1:A2"]), omitted()],
        E(Value),
        Choice("an omitted argument is #VALUE!")
    );
    agg!(
        r,
        s,
        "SUMPRODUCT",
        [reference(s, &["A1", "A7"])],
        E(Value),
        Measured(
            "dc-bd5: a reference of several areas is #VALUE! (Excel 16.0.20430, cfunc_m3_4's capture)"
        )
    );
    agg!(
        r,
        s,
        "SUMPRODUCT",
        [reference(s, &["A1:A3"]), reference(s, &["C1:C2"])],
        E(Value),
        Documented(
            "arrays of different dimensions are #VALUE!, here a reference of three rows and one of two"
        )
    );
    agg!(
        r,
        s,
        "SUMPRODUCT",
        [reference(s, &["A1:A2"]), one_two()],
        E(Value),
        Documented(
            "arrays of different dimensions are #VALUE!, here a column of two cells and a row of two entries"
        )
    );
    agg!(
        r,
        s,
        "SUMPRODUCT",
        [
            reference(s, &["C1:C2"]),
            array_arg(
                "{1;2}",
                vec![vec![CalcValue::number(1.0)], vec![CalcValue::number(2.0)]]
            )
        ],
        N(30.0),
        Documented(
            "SUMPRODUCT multiplies corresponding entries and adds the products, here of a column of two cells and a column of two entries"
        )
    );
    agg!(
        r,
        s,
        "SUMPRODUCT",
        [num("1E-200"), num("1E-200")],
        N(0.0),
        Measured(
            "dc-bd5: a product below the normal range is 0 (Excel 16.0.20430, cfunc_m3_4's capture)"
        )
    );

    // COUNT.
    agg!(
        r,
        s,
        "COUNT",
        [mixed(s)],
        N(2.0),
        Documented(
            "only the numbers of a reference are counted; its empty cells, logicals, text and errors are not"
        )
    );
    agg!(
        r,
        s,
        "COUNT",
        [with_error(s)],
        N(2.0),
        Documented("an error value in a reference is not counted")
    );
    agg!(
        r,
        s,
        "COUNT",
        [reference(s, &["A1:A2", "B2:B3"])],
        N(2.0),
        Choice(
            "only the numbers of each area are counted: the page states the rule for an array or a reference and is silent on a reference of several areas"
        )
    );
    agg!(
        r,
        s,
        "COUNT",
        [num("1"), txt("2"), logical(true)],
        N(3.0),
        Documented("numbers, number text and logicals typed as arguments are counted")
    );
    agg!(
        r,
        s,
        "COUNT",
        [txt("a"), err(NA), txt("")],
        N(0.0),
        Documented("errors and text that cannot be read as a number are not counted")
    );
    agg!(
        r,
        s,
        "COUNT",
        [typed_array()],
        N(1.0),
        Documented("only the numbers of an array are counted")
    );
    agg!(
        r,
        s,
        "COUNT",
        [num("1"), omitted()],
        N(2.0),
        Choice("an omitted argument is the number 0, and counts")
    );
    agg!(
        r,
        s,
        "COUNT",
        [txt("1/2")],
        Refused(Some(0)),
        Unsupported("typed text that Excel's number-text rules may read as a date (dc-uar)")
    );

    // COUNTA.
    agg!(
        r,
        s,
        "COUNTA",
        [mixed(s)],
        N(7.0),
        Documented("every cell with content is counted, empty text included; an empty cell is not")
    );
    agg!(
        r,
        s,
        "COUNTA",
        [with_error(s)],
        N(3.0),
        Documented("error values are counted")
    );
    agg!(
        r,
        s,
        "COUNTA",
        [reference(s, &["A5"])],
        N(0.0),
        Documented("an empty cell is not counted")
    );
    agg!(
        r,
        s,
        "COUNTA",
        [txt("a"), err(NA), logical(false), txt("")],
        N(4.0),
        Choice("every value typed as an argument is counted; the page is silent")
    );
    agg!(
        r,
        s,
        "COUNTA",
        [typed_array()],
        N(3.0),
        Choice("every element of an array is counted; the page is silent")
    );
    agg!(
        r,
        s,
        "COUNTA",
        [num("1"), omitted()],
        N(2.0),
        Choice("an omitted argument is the number 0, and counts")
    );

    // COUNTBLANK.
    agg!(
        r,
        s,
        "COUNTBLANK",
        [reference(s, &["A5"])],
        N(1.0),
        Documented("an empty cell is counted")
    );
    agg!(
        r,
        s,
        "COUNTBLANK",
        [with_error(s)],
        N(0.0),
        Documented("only empty cells and cells whose formula gives empty text are counted")
    );
    agg!(
        r,
        s,
        "COUNTBLANK",
        [mixed(s)],
        N(2.0),
        Choice(
            "the empty A5 counts (documented), and so does A6, whose empty text is a constant, as the page counts a formula's empty text"
        )
    );
    agg!(
        r,
        s,
        "COUNTBLANK",
        [reference(s, &["A1:C8"])],
        N(7.0),
        Choice("A5, A6 and the empty B4:B8 count; A6 holds empty text as a constant")
    );
    agg!(
        r,
        s,
        "COUNTBLANK",
        [reference(s, &["A5", "A6"])],
        E(Value),
        Measured(
            "dc-bd5: a reference of several areas is #VALUE! (Excel 16.0.20430, cfunc_m3_4's capture)"
        )
    );
    agg!(
        r,
        s,
        "COUNTBLANK",
        [err(Ref)],
        E(Ref),
        Measured(
            "dc-ifc: #REF! given to COUNTBLANK is the result (Excel 16.0.20430, 2026-10-27: ERROR.TYPE(COUNTBLANK(#REF!)) is 4)"
        )
    );

    // AVERAGE.
    agg!(
        r,
        s,
        "AVERAGE",
        [num("1"), num("2"), num("5")],
        N(8.0 / 3.0),
        Documented("the arithmetic mean of its numbers: 8 divided by 3, rounded once")
    );
    agg!(
        r,
        s,
        "AVERAGE",
        [mixed(s)],
        N(2.5),
        Documented("the text, logicals and empty cells of a reference are ignored")
    );
    agg!(
        r,
        s,
        "AVERAGE",
        [reference(s, &["A7", "B3"])],
        N(0.5),
        Choice(
            "the numbers of every area are averaged: the page states the rule for an array or a reference and is silent on a reference of several areas"
        )
    );
    agg!(
        r,
        s,
        "AVERAGE",
        [with_error(s)],
        E(Div0),
        Documented(ERRORS_ERROR)
    );
    agg!(
        r,
        s,
        "AVERAGE",
        [txt("a")],
        E(Value),
        Documented(UNREADABLE_TEXT)
    );
    agg!(
        r,
        s,
        "AVERAGE",
        [txt("2"), logical(true)],
        N(1.5),
        Choice(
            "divergent from the AVERAGE page, which says logicals and number text typed as arguments are not counted: this kernel counts them, as the MIN, MAX, COUNT, SUMSQ and AVERAGEA pages state for those functions"
        )
    );
    agg!(
        r,
        s,
        "AVERAGE",
        [reference(s, &["A3:A6"])],
        E(Div0),
        Choice("with nothing counted the total is divided by 0, #DIV/0!; the page is silent")
    );
    agg!(
        r,
        s,
        "AVERAGE",
        [typed_array()],
        N(1.0),
        Choice("an array's elements follow the rule for a reference's cells")
    );
    agg!(
        r,
        s,
        "AVERAGE",
        [num("1"), omitted()],
        N(0.5),
        Choice("an omitted argument is the number 0, and counts")
    );
    agg!(
        r,
        s,
        "AVERAGE",
        [num("9.99999999999999E307"), num("9.99999999999999E307")],
        E(Num),
        Choice("a total that overflows is #NUM!")
    );

    // AVERAGEA.
    agg!(
        r,
        s,
        "AVERAGEA",
        [txt("2"), logical(true)],
        N(1.5),
        Documented(TYPED_COUNTED)
    );
    agg!(
        r,
        s,
        "AVERAGEA",
        [reference(s, &["A3", "A8"])],
        N(0.5),
        Choice(
            "each area's TRUE is 1 and its FALSE 0: the page states the rule for an array or a reference and is silent on a reference of several areas"
        )
    );
    agg!(
        r,
        s,
        "AVERAGEA",
        [mixed(s)],
        N(6.0 / 7.0),
        Choice(
            "a reference's text and empty text are 0, as the page's worked example computes although another remark there calls text ignored; TRUE is 1, FALSE 0 and the empty A5 is skipped: 6 divided by 7, rounded once"
        )
    );
    agg!(
        r,
        s,
        "AVERAGEA",
        [reference(s, &["A5"])],
        E(Div0),
        Choice("an empty cell is ignored (documented); with nothing counted the result is #DIV/0!")
    );
    agg!(
        r,
        s,
        "AVERAGEA",
        [txt("a")],
        E(Value),
        Documented(UNREADABLE_TEXT)
    );
    agg!(
        r,
        s,
        "AVERAGEA",
        [with_error(s)],
        E(Div0),
        Documented(ERRORS_ERROR)
    );
    agg!(
        r,
        s,
        "AVERAGEA",
        [typed_array()],
        N(0.5),
        Measured(
            "dc-bd5: in an array AVERAGEA reads text as 0 and passes over logicals: AVERAGEA({1,\"2\",TRUE}) is 0.5 (Excel 16.0.20430, cfunc_m3_4's capture; 16.0.20527 gives AVERAGEA({1,TRUE}) 1)"
        )
    );

    // MIN and MAX.
    agg!(
        r,
        s,
        "MIN",
        [mixed(s)],
        N(1.0),
        Documented(
            "only the numbers of a reference are used; its empty cells, logicals and text are ignored"
        )
    );
    agg!(
        r,
        s,
        "MIN",
        [txt("2"), logical(true)],
        N(1.0),
        Documented(TYPED_COUNTED)
    );
    agg!(
        r,
        s,
        "MIN",
        [reference(s, &["A3:A6"])],
        N(0.0),
        Documented("with no numbers the result is 0")
    );
    agg!(
        r,
        s,
        "MIN",
        [with_error(s)],
        E(Div0),
        Documented(ERRORS_ERROR)
    );
    agg!(
        r,
        s,
        "MIN",
        [num("5"), txt("a")],
        E(Value),
        Documented(UNREADABLE_TEXT)
    );
    agg!(
        r,
        s,
        "MIN",
        [array_arg(
            "{TRUE,\"1\"}",
            vec![vec![CalcValue::logical(true), text_value("1")]]
        )],
        N(0.0),
        Documented("only the numbers of an array are used, and with none the result is 0")
    );
    agg!(
        r,
        s,
        "MIN",
        [num("1"), omitted()],
        N(0.0),
        Choice(OMITTED_ZERO)
    );
    agg!(
        r,
        s,
        "MAX",
        [mixed(s)],
        N(4.0),
        Documented(
            "only the numbers of a reference are used; its empty cells, logicals and text are ignored"
        )
    );
    agg!(
        r,
        s,
        "MAX",
        [txt("2"), logical(true)],
        N(2.0),
        Documented(TYPED_COUNTED)
    );
    agg!(
        r,
        s,
        "MAX",
        [reference(s, &["A4", "B3"])],
        N(-3.0),
        Choice(
            "only the numbers of each area are used, so its text is not 0: the page states the rule for an array or a reference and is silent on a reference of several areas"
        )
    );
    agg!(
        r,
        s,
        "MAX",
        [with_error(s)],
        E(Div0),
        Documented(ERRORS_ERROR)
    );
    agg!(
        r,
        s,
        "MAX",
        [reference(s, &["A3:A6"])],
        N(0.0),
        Documented("with no numbers the result is 0")
    );
    agg!(
        r,
        s,
        "MAX",
        [num("-1"), omitted()],
        N(0.0),
        Choice(OMITTED_ZERO)
    );

    // MINA and MAXA.
    agg!(
        r,
        s,
        "MINA",
        [reference(s, &["A3", "A7"])],
        N(1.0),
        Choice(
            "each area's TRUE is 1: the page states the rule for an array or a reference and is silent on a reference of several areas"
        )
    );
    agg!(
        r,
        s,
        "MINA",
        [txt("2"), logical(true)],
        N(1.0),
        Documented(TYPED_COUNTED)
    );
    agg!(
        r,
        s,
        "MINA",
        [reference(s, &["A1:A4"])],
        N(0.0),
        Choice(
            "a reference's text is 0, as one remark of the page says while another calls it ignored"
        )
    );
    agg!(
        r,
        s,
        "MINA",
        [reference(s, &["A5"])],
        N(0.0),
        Documented("an empty cell is ignored, and with no values the result is 0")
    );
    agg!(
        r,
        s,
        "MINA",
        [with_error(s)],
        E(Div0),
        Documented(ERRORS_ERROR)
    );
    agg!(
        r,
        s,
        "MAXA",
        [reference(s, &["A3", "B3"])],
        N(1.0),
        Choice(
            "each area's TRUE is 1: the page states the rule for an array or a reference and is silent on a reference of several areas"
        )
    );
    agg!(
        r,
        s,
        "MAXA",
        [txt("2"), logical(true)],
        N(2.0),
        Documented(TYPED_COUNTED)
    );
    agg!(
        r,
        s,
        "MAXA",
        [reference(s, &["A4", "B3"])],
        N(0.0),
        Choice(
            "a reference's text is 0, which exceeds -3; were it ignored, as another remark of the page says, the result would be -3"
        )
    );
    agg!(
        r,
        s,
        "MAXA",
        [txt("a")],
        E(Value),
        Documented(UNREADABLE_TEXT)
    );
    agg!(
        r,
        s,
        "MAXA",
        [array_arg(
            "{-1,\"a\"}",
            vec![vec![CalcValue::number(-1.0), text_value("a")]]
        )],
        N(-1.0),
        Measured(
            "dc-bd5: MAXA passes over an array's text and logicals: MAXA({-1,\"a\"}) is -1 (Excel 16.0.20430, cfunc_m3_4's capture)"
        )
    );
    rows
}

/// The pairs `(id, spelling)` of the per-ID manifest in the migration record: each function's
/// object there starts `{"id": "<family>.<member>", "spelling": "<SPELLING>"`.
fn manifest_ids(record: &str) -> BTreeSet<(String, String)> {
    const ID: &str = "{\"id\": \"";
    const SPELLING: &str = "\", \"spelling\": \"";
    let mut ids = BTreeSet::new();
    let mut rest = record;
    while let Some(at) = rest.find(ID) {
        rest = &rest[at + ID.len()..];
        let Some(end) = rest.find('"') else { break };
        let id = rest[..end].to_owned();
        if let Some(spelling) = rest[end..].strip_prefix(SPELLING) {
            let close = spelling.find('"').expect("a spelling ends");
            assert!(
                ids.insert((id, spelling[..close].to_owned())),
                "the per-ID manifest repeats an entry"
            );
        }
    }
    ids
}

#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test)]
fn cfunc_m3_4__argument_classes() {
    const CASE: &str = "cfunc_m3_4__argument_classes";
    let fast = NumericMode::Fast;
    let mut sheet = argument_classes_sheet();
    note!(
        "ACCEPT-NOTE: {CASE}: the sheet holds {}; A5 is blank",
        sheet.described()
    );
    let rows = argument_classes_rows(&mut sheet, false);
    let s = &mut sheet;

    // Metadata and dispatch come from the one generated catalogue: family 4 holds exactly these
    // entries, each eager, with Excel's arity, its parameter classes and its signature help.
    let family: BTreeSet<(u32, String)> = catalogue()
        .targets()
        .filter(|target| target.id().raw() >> 16 == 4)
        .map(|target| (target.id().raw(), target.spelling().to_owned()))
        .collect();
    let expected: BTreeSet<(u32, String)> = AGGREGATES
        .iter()
        .map(|(spelling, member)| ((4 << 16) | member, (*spelling).to_owned()))
        .collect();
    assert_eq!(family, expected, "the aggregate family in the catalogue");
    for (spelling, _) in AGGREGATES {
        let target = surface(Notation::Call, spelling);
        assert_eq!(
            target.argument_demand(),
            ArgumentDemand::Eager,
            "{spelling}"
        );
        let arity = target.arity();
        if spelling == "COUNTBLANK" {
            assert_eq!((arity.min, arity.max), (1, 1));
            assert_eq!(
                target.parameter_class(0),
                Some(ParameterClass::ValueOrReference)
            );
        } else {
            assert_eq!((arity.min, arity.max), (1, 255), "{spelling}");
            for index in [0, 1, 254] {
                assert_eq!(
                    target.parameter_class(index),
                    Some(ParameterClass::ValueOrReference),
                    "{spelling} argument {index}"
                );
            }
        }
    }
    let helps = [
        ("SUM", "SUM(number1, [number2], ...)"),
        ("PRODUCT", "PRODUCT(number1, [number2], ...)"),
        ("SUMPRODUCT", "SUMPRODUCT(array1, [array2], ...)"),
        ("COUNT", "COUNT(value1, [value2], ...)"),
        ("COUNTBLANK", "COUNTBLANK(range)"),
        ("AVERAGEA", "AVERAGEA(value1, [value2], ...)"),
        ("MAX", "MAX(number1, [number2], ...)"),
    ];
    for (spelling, display) in helps {
        assert_eq!(
            surface(Notation::Call, spelling).signature_help().display,
            display
        );
    }
    let manifest = manifest_ids(AGGREGATE_RECORD);
    let listed: BTreeSet<(String, String)> = AGGREGATES
        .iter()
        .map(|(spelling, member)| (format!("4.{member}"), (*spelling).to_owned()))
        .collect();
    assert_eq!(
        manifest, listed,
        "the per-ID manifest of migration/ports/cfunc-m3.4.json against the catalogue"
    );
    note!(
        "ACCEPT-NOTE: {CASE}: the catalogue's family 4 holds exactly the {} entries {:?}, each eager; COUNTBLANK takes one value or reference and the others 1 to 255; the per-ID manifest of migration/ports/cfunc-m3.4.json lists the same IDs and spellings",
        family.len(),
        AGGREGATES.map(|(spelling, _)| spelling)
    );

    // The W111 count-family and reference-vs-array readings establish #VALUE! for computed
    // scalars. Retained fn2 q187 also establishes #VALUE! for an omitted optional LAMBDA
    // parameter. The kernel still leaves array lifting to calc-fml.
    let countblank = surface(Notation::Call, "COUNTBLANK");
    for value in [
        CalcValue::number(1.0),
        CalcValue::logical(true),
        CalcValue::text(ExcelText::from_interop_assignment("")),
        CalcValue::empty(),
        CalcValue::missing(),
    ] {
        assert_eq!(
            run(countblank, &mut Recorded::new(vec![value.clone()]), &fast),
            Ok(CalcValue::error(WorksheetErrorCode::Value)),
            "COUNTBLANK({value:?})"
        );
    }
    for refused in [
        CalcValue::array(CalcArray::from_scalar(CalcValue::empty())),
    ] {
        let mut given = Recorded::new(vec![refused.clone()]);
        assert_eq!(
            run(countblank, &mut given, &fast),
            Err(FunctionFault::ReferenceRequired {
                id: countblank.id(),
                index: 0
            }),
            "COUNTBLANK({refused:?})"
        );
    }

    // Through the call interface alone (no formula writes a direct blank): a direct Empty reads as
    // an omitted argument does, the number 0 (an unmeasured choice).
    let direct_empty: [(&str, Vec<CalcValue>, Result<f64, WorksheetErrorCode>); 7] = [
        ("COUNT", vec![CalcValue::empty()], Ok(1.0)),
        ("COUNTA", vec![CalcValue::empty()], Ok(1.0)),
        (
            "AVERAGE",
            vec![CalcValue::empty(), CalcValue::number(2.0)],
            Ok(1.0),
        ),
        (
            "MIN",
            vec![CalcValue::empty(), CalcValue::number(2.0)],
            Ok(0.0),
        ),
        (
            "MAXA",
            vec![CalcValue::empty(), CalcValue::number(-2.0)],
            Ok(0.0),
        ),
        (
            "PRODUCT",
            vec![CalcValue::empty(), CalcValue::number(2.0)],
            Ok(0.0),
        ),
        (
            "SUMPRODUCT",
            vec![CalcValue::empty()],
            Err(WorksheetErrorCode::Value),
        ),
    ];
    for (spelling, values, expected) in direct_empty {
        let target = surface(Notation::Call, spelling);
        let result = run(target, &mut Recorded::new(values), &fast);
        let observed = result
            .as_ref()
            .map(|value| value.as_number().ok_or(value.as_error()));
        assert_eq!(
            observed,
            Ok(expected.map_err(Some)),
            "{spelling} on a direct Empty: {result:?}"
        );
    }

    // An array held inside an array argument (a GROUPBY result's cell, dc-h9u; Excel 16.0.20527,
    // 2026-10-30, through COM): SUM, COUNT and COUNTA read through it, SUMPRODUCT takes it as no
    // number, the others give #N/A. Measured: catalogue sheet dch9u_functions.
    let nested = CalcValue::array(
        CalcArray::from_rows(vec![vec![
            CalcValue::number(1.0),
            CalcValue::array(CalcArray::from_scalar(CalcValue::number(2.0))),
        ]])
        .expect("a 1-by-2 array"),
    );
    for (spelling, _) in AGGREGATES
        .iter()
        .filter(|(spelling, _)| *spelling != "COUNTBLANK")
    {
        let target = surface(Notation::Call, spelling);
        let expected = match *spelling {
            "SUM" => CalcValue::number(3.0),
            "COUNT" | "COUNTA" => CalcValue::number(2.0),
            "SUMPRODUCT" => CalcValue::number(1.0),
            _ => CalcValue::error(WorksheetErrorCode::NA),
        };
        assert_eq!(
            run(target, &mut Recorded::new(vec![nested.clone()]), &fast),
            Ok(expected),
            "{spelling} of an array holding an array"
        );
    }
    // Values that no formula writes, each refused naming its argument: a NaN or an infinity
    // given to a count.
    for spelling in ["COUNT", "COUNTA"] {
        let target = surface(Notation::Call, spelling);
        for n in [f64::NAN, f64::INFINITY] {
            assert_eq!(
                run(
                    target,
                    &mut Recorded::new(vec![CalcValue::number(1.0), CalcValue::number(n)]),
                    &fast
                ),
                Err(FunctionFault::NotImplemented {
                    id: target.id(),
                    argument: Some(1)
                }),
                "{spelling}(1, {n:?})"
            );
        }
    }

    // A reference with no cell reader, or one the reader refuses, ends the call in
    // ReferenceUnreadable naming the argument, never in blanks.
    let range = reference(s, &["A1:A8"]).value;
    let mut unreadable = 0;
    for (spelling, _) in AGGREGATES {
        let target = surface(Notation::Call, spelling);
        let (values, index) = if spelling == "COUNTBLANK" {
            (vec![range.clone()], 0)
        } else {
            (vec![CalcValue::number(1.0), range.clone()], 1)
        };
        let fault = Err(FunctionFault::ReferenceUnreadable {
            id: target.id(),
            index,
        });
        assert_eq!(
            run(target, &mut Recorded::new(values.clone()), &fast),
            fault,
            "{spelling} without a cell reader"
        );
        s.refuse = true;
        let refused = invoke(
            target,
            &mut Recorded::new(values),
            &mut EvalServices::with_cells(&fast, s),
            &mut FunctionCallScratch::new(),
        );
        s.refuse = false;
        assert_eq!(refused, fault, "{spelling} over a refusing reader");
        unreadable += 2;
    }
    note!(
        "ACCEPT-NOTE: {CASE}: through the call interface alone: COUNTBLANK gives #VALUE! for computed scalars and refuses an array and an omitted argument with ReferenceRequired; the other aggregates' direct Empty cases remain unmeasured; each aggregate refuses an array held in an array naming it, COUNT and COUNTA refuse NaN and infinity naming them; {unreadable} calls over a reference without a reader or with a refusing one ended in ReferenceUnreadable naming the argument"
    );

    // NumericMode reaches every arithmetic site. Under Exact each call that adds, multiplies,
    // divides or converts number text is refused; the counts and the extremes, which choose and
    // count without rounding, compute the value of their Fast row, and so does an average or a
    // product that reaches no arithmetic site.
    let exact = exact_mode();
    let mut exact_refused = 0;
    for (spelling, args) in [
        ("SUM", vec![reference(s, &["A1:A8"])]),
        ("PRODUCT", vec![reference(s, &["A1:A8"])]),
        ("SUMSQ", vec![reference(s, &["A1:A8"])]),
        (
            "SUMPRODUCT",
            vec![reference(s, &["A1:A8"]), reference(s, &["C1:C8"])],
        ),
        // One argument: each entry is added with no multiplication before it.
        ("SUMPRODUCT", vec![reference(s, &["C1:C8"])]),
        ("AVERAGE", vec![reference(s, &["A1:A8"])]),
        ("AVERAGEA", vec![reference(s, &["A1:A8"])]),
        ("MIN", vec![txt("2"), logical(true)]),
    ] {
        let target = surface(Notation::Call, spelling);
        let result = invoke(
            target,
            &mut Recorded::new(args.iter().map(|a| a.value.clone()).collect()),
            &mut EvalServices::with_cells(&exact, s),
            &mut FunctionCallScratch::new(),
        );
        assert_eq!(
            result,
            Err(FunctionFault::ExactUnavailable { id: target.id() }),
            "{spelling} under Exact"
        );
        exact_refused += 1;
    }
    let mut exact_computed = 0;
    for (spelling, args) in [
        ("COUNT", vec![reference(s, &["A1:A8"])]),
        ("COUNT", vec![num("1"), txt("2"), logical(true)]),
        ("COUNTA", vec![reference(s, &["A1:A8"])]),
        ("COUNTBLANK", vec![reference(s, &["A1:A8"])]),
        ("MIN", vec![reference(s, &["A1:A8"])]),
        ("MAX", vec![reference(s, &["A1:A8"])]),
        ("MINA", vec![reference(s, &["A1:A4"])]),
        ("MAXA", vec![reference(s, &["A4", "B3"])]),
        ("AVERAGE", vec![reference(s, &["A3:A6"])]),
        ("PRODUCT", vec![reference(s, &["A3:A6"])]),
    ] {
        let target = surface(Notation::Call, spelling);
        let texts: Vec<&str> = args.iter().map(|a| a.excel.as_str()).collect();
        let formula = target.oracle_formula(&texts).expect("a formula");
        let under_fast = row_value(&rows, &formula);
        let under_exact = invoke(
            target,
            &mut Recorded::new(args.iter().map(|a| a.value.clone()).collect()),
            &mut EvalServices::with_cells(&exact, s),
            &mut FunctionCallScratch::new(),
        );
        assert!(
            under_exact
                .as_ref()
                .is_ok_and(|value| identical(value, under_fast)),
            "{formula} under Exact gives {under_exact:?}, under Fast {under_fast:?}"
        );
        exact_computed += 1;
    }
    note!(
        "ACCEPT-NOTE: {CASE}: under NumericMode::Exact {exact_refused} calls that round (sums, products, averages over A1:A8, SUMPRODUCT(C1:C8), which adds with no multiplication, and MIN reading number text) were refused with ExactUnavailable, and {exact_computed} calls that reach no arithmetic site (counts, extremes, an average and a product of nothing) computed the value of their Fast oracle row"
    );

    // A reader may also visit a blank cell, as Empty: over such a reader every count, blank
    // count and average gives the value of its Fast oracle row.
    let mut dense_calls = 0;
    for (spelling, areas) in [
        ("COUNT", &[&["A1:A8"][..]][..]),
        ("COUNTA", &[&["A1:A8"][..]][..]),
        ("COUNTA", &[&["A5"][..]][..]),
        ("COUNTBLANK", &[&["A1:A8"][..]][..]),
        ("COUNTBLANK", &[&["A1:C8"][..]][..]),
        ("AVERAGE", &[&["A1:A8"][..]][..]),
        ("AVERAGEA", &[&["A1:A8"][..]][..]),
        ("AVERAGEA", &[&["A5"][..]][..]),
        ("MINA", &[&["A5"][..]][..]),
        ("MAXA", &[&["A4", "B3"][..]][..]),
        ("PRODUCT", &[&["A3:A6"][..]][..]),
        ("SUMPRODUCT", &[&["A1:A8"][..], &["C1:C8"][..]][..]),
    ] {
        let target = surface(Notation::Call, spelling);
        let args: Vec<Arg> = areas.iter().map(|texts| reference(s, texts)).collect();
        let texts: Vec<&str> = args.iter().map(|a| a.excel.as_str()).collect();
        let formula = target.oracle_formula(&texts).expect("a formula");
        let mut dense = DenseReader(s);
        let visited_blanks = invoke(
            target,
            &mut Recorded::new(args.iter().map(|a| a.value.clone()).collect()),
            &mut EvalServices::with_cells(&fast, &mut dense),
            &mut FunctionCallScratch::new(),
        );
        let under_fast = row_value(&rows, &formula);
        assert!(
            visited_blanks
                .as_ref()
                .is_ok_and(|value| identical(value, under_fast)),
            "{formula} over a reader that visits blank cells gives {visited_blanks:?}, the row {under_fast:?}"
        );
        dense_calls += 1;
    }
    note!(
        "ACCEPT-NOTE: {CASE}: {dense_calls} calls over a reader that also visits every blank cell as Empty gave the values of their Fast oracle rows"
    );

    // A reader that breaks the CellReader contract cannot supply the area, and the call ends in
    // ReferenceUnreadable naming the argument: a visit outside the area asked for, cells out of
    // row-major order where the order places each entry (SUMPRODUCT), and more cells than the
    // area holds where their number is subtracted from its size (COUNTBLANK). Each call first
    // gives a value over the test sheet's reader, which keeps the contract.
    let breaches: [(&str, &str, Breach, f64); 4] = [
        ("SUM", "A1:A2", Breach::Outside("C1"), 1.0),
        ("COUNT", "A1:A2", Breach::Outside("C1"), 1.0),
        ("SUMPRODUCT", "C6:C7", Breach::Reversed, 10.5),
        ("COUNTBLANK", "A1", Breach::Twice, 0.0),
    ];
    for (spelling, area, breach, kept) in breaches {
        let target = surface(Notation::Call, spelling);
        let values = vec![reference(s, &[area]).value];
        let honest = invoke(
            target,
            &mut Recorded::new(values.clone()),
            &mut EvalServices::with_cells(&fast, s),
            &mut FunctionCallScratch::new(),
        );
        assert_eq!(
            honest
                .as_ref()
                .map(|value| value.as_number().map(f64::to_bits)),
            Ok(Some(kept.to_bits())),
            "{spelling}({area}) over the test sheet's reader: {honest:?}"
        );
        let mut breaching = BreachingReader { sheet: s, breach };
        let broken = invoke(
            target,
            &mut Recorded::new(values),
            &mut EvalServices::with_cells(&fast, &mut breaching),
            &mut FunctionCallScratch::new(),
        );
        assert_eq!(
            broken,
            Err(FunctionFault::ReferenceUnreadable {
                id: target.id(),
                index: 0
            }),
            "{spelling}({area}) over a reader that breaks the contract by {breach:?}"
        );
        note!(
            "ACCEPT-NOTE: {CASE}: {spelling}({area}) = {kept} over the test sheet's reader, and ReferenceUnreadable (argument 0) over a reader that breaks the CellReader contract by {breach:?}"
        );
    }

    // The oracle workbook: this case's sheet holds exactly its rows and cells, and the committed
    // workbook is its generator's output.
    assert_workbook_sheet(AGGREGATE_WORKBOOK, AGGREGATE_WORKBOOK_PATH, &rows, s);
    #[cfg(all(windows, not(target_arch = "wasm32")))]
    {
        let generated = generated_fixtures();
        let names: Vec<&str> = generated.keys().map(String::as_str).collect();
        assert_eq!(
            names, GENERATED_WORKBOOKS,
            "the generator writes the oracle workbooks and nothing else"
        );
        let regenerated = &generated["cfunc_m3_4/oracle.xlsx"];
        assert!(
            regenerated.as_slice() == AGGREGATE_WORKBOOK,
            "the committed aggregate oracle workbook is not the generator's output (sha256 {} committed, {} generated)",
            sha256_hex(AGGREGATE_WORKBOOK),
            sha256_hex(regenerated)
        );
        note!(
            "ACCEPT-NOTE: {CASE}: crates/calc-func/tests/fixtures/make-fixtures.ps1, run now under Windows PowerShell, wrote the committed aggregate oracle workbook byte for byte ({} bytes)",
            regenerated.len()
        );
    }
    #[cfg(not(all(windows, not(target_arch = "wasm32"))))]
    {
        note!(
            "ACCEPT-NOTE: {CASE}: the generator runs under Windows PowerShell, so the check that the committed aggregate oracle workbook is its output runs on the windows route; here the workbook is checked against the rows only"
        );
    }
    aggregate_rows_note(&rows);
}

// ---------------------------------------------------------------------------------------------
// cfunc_m3_4__sparse_blanks

/// The cells the sparse rows read: column A holds five cells down to its last row (A2 holds
/// empty text), column B two, and column C one; every other cell is blank.
fn sparse_blanks_sheet() -> TestSheet {
    TestSheet::new(&[
        ("A1", "3", CalcValue::number(3.0)),
        ("A2", "empty text", text_value("")),
        ("A3", "text \"x\"", text_value("x")),
        ("A1000", "5", CalcValue::number(5.0)),
        ("A1048576", "7", CalcValue::number(7.0)),
        ("B1", "2", CalcValue::number(2.0)),
        ("B1048576", "10", CalcValue::number(10.0)),
        ("C500", "TRUE", CalcValue::logical(true)),
    ])
}

/// Above this many bytes, a call over a whole column would be materializing it: a column of
/// 1,048,576 binary64 values alone takes 8 MiB.
const SPARSE_BYTES: u64 = 64 * 1024;

/// Runs a sparse row over the whole columns `columns` and checks, by the test sheet's own
/// instruments, that the call asked the reader for exactly those areas, once each, and was
/// visited only with their held cells; then calls it again and counts the bytes this thread
/// allocated during the call. Returns the cells of the areas, the cells visited and the bytes.
fn sparse_row(
    rows: &mut Rows,
    s: &mut TestSheet,
    spelling: &'static str,
    columns: &[&str],
    expect: Expect,
    basis: Basis,
) -> (u64, usize, u64) {
    s.reads.clear();
    s.visited = 0;
    let args: Vec<Arg> = columns
        .iter()
        .map(|column| reference(s, &[*column]))
        .collect();
    aggregate_row(rows, s, named(spelling), &args, expect.clone(), basis);
    let areas: Vec<GridArea> = columns.iter().map(|column| s.area(column)).collect();
    let held = s
        .cells
        .keys()
        .filter(|(row, column)| {
            let key = CellKey::new(s.id, *row, *column).expect("a held cell");
            areas.iter().any(|area| area.contains(key))
        })
        .count();
    assert_eq!(
        (s.reads.as_slice(), s.visited),
        (areas.as_slice(), held),
        "{spelling}({}) asks for each column once and is visited only with its held cells",
        columns.join(",")
    );
    let cells: u64 = areas.iter().map(|area| area.cell_count()).sum();
    let mut again = Recorded::new(args.iter().map(|a| a.value.clone()).collect());
    let target = surface(Notation::Call, spelling);
    let fast = NumericMode::Fast;
    let (result, bytes) = bytes_allocated_during(|| {
        black_box(invoke(
            target,
            &mut again,
            &mut EvalServices::with_cells(&fast, s),
            &mut FunctionCallScratch::new(),
        ))
    });
    assert!(
        expect.holds(target.id(), &result),
        "{spelling} again: {result:?}"
    );
    assert!(
        bytes < SPARSE_BYTES,
        "{spelling}({}) allocated {bytes} bytes over {cells} cells",
        columns.join(",")
    );
    (cells, held, bytes)
}

#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test)]
fn cfunc_m3_4__sparse_blanks() {
    const CASE: &str = "cfunc_m3_4__sparse_blanks";
    let mut sheet = sparse_blanks_sheet();
    note!(
        "ACCEPT-NOTE: {CASE}: the sheet holds {}; every other cell is blank",
        sheet.described()
    );
    let s = &mut sheet;
    let mut rows = Rows::new(CASE, "sparse_blanks", false);
    let r = &mut rows;
    let mut measured = Vec::new();
    let mut measure = |r: &mut Rows,
                       s: &mut TestSheet,
                       spelling,
                       columns: &[&str],
                       expect,
                       basis| {
        let (cells, visited, bytes) = sparse_row(r, s, spelling, columns, expect, basis);
        measured.push(format!(
                "{spelling}({}): 1 read per column, {cells} cells, {visited} visited, {bytes} bytes allocated",
                columns.join(",")
            ));
    };

    // COUNTBLANK's blank cardinality is the area's size less the cells visited with a value that
    // is not empty text.
    measure(
        r,
        s,
        "COUNTBLANK",
        &["B:B"],
        N(1_048_574.0),
        Documented("an empty cell is counted"),
    );
    measure(
        r,
        s,
        "COUNTBLANK",
        &["A:A"],
        N(1_048_572.0),
        Choice("A2's empty text is a constant, counted as the page counts a formula's empty text"),
    );
    measure(
        r,
        s,
        "COUNTBLANK",
        &["A:C"],
        N(3_145_721.0),
        Choice(
            "three whole columns less the 7 cells with a value; A2's constant empty text counts",
        ),
    );
    measure(
        r,
        s,
        "COUNT",
        &["A:A"],
        N(3.0),
        Documented("only the numbers of a reference are counted"),
    );
    measure(
        r,
        s,
        "COUNTA",
        &["A:A"],
        N(5.0),
        Documented("every cell with content is counted, empty text included"),
    );
    measure(
        r,
        s,
        "SUM",
        &["A:A"],
        N(15.0),
        Documented("SUM ignores the text of the cells it reads"),
    );
    measure(
        r,
        s,
        "PRODUCT",
        &["A:A"],
        N(105.0),
        Documented("only the numbers of a reference are multiplied"),
    );
    measure(
        r,
        s,
        "SUMSQ",
        &["A:A"],
        N(83.0),
        Documented("only the numbers of a reference count"),
    );
    measure(
        r,
        s,
        "AVERAGE",
        &["A:A"],
        N(5.0),
        Documented("the text and empty cells of a reference are ignored"),
    );
    measure(
        r,
        s,
        "AVERAGEA",
        &["A:A"],
        N(3.0),
        Choice(
            "a reference's text and empty text are 0 and its empty cells are skipped: 15 over 5",
        ),
    );
    measure(
        r,
        s,
        "MIN",
        &["A:A"],
        N(3.0),
        Documented("only the numbers of a reference are used"),
    );
    measure(
        r,
        s,
        "MAX",
        &["A:A"],
        N(7.0),
        Documented("only the numbers of a reference are used"),
    );
    measure(
        r,
        s,
        "MINA",
        &["A:A"],
        N(0.0),
        Choice("a reference's text is 0, as one remark of the page says"),
    );
    measure(
        r,
        s,
        "MAXA",
        &["B:B"],
        N(10.0),
        Documented("the empty cells of a reference are ignored"),
    );
    measure(
        r,
        s,
        "SUMPRODUCT",
        &["A:A", "B:B"],
        N(76.0),
        Documented("a non-numeric entry, a blank cell included, is treated as 0"),
    );
    for line in &measured {
        note!("ACCEPT-NOTE: {CASE}: instrumented reader: {line}");
    }

    // The whole sheet, through the call interface alone (no formula on the sheet can read all of
    // it without reading itself): 2^34 cells, of which the reader visits the 8 held.
    let whole = GridArea::full_sheet(s.id);
    let whole_sheet = CalcValue::reference(ReferenceLike::area(whole));
    let fast = NumericMode::Fast;
    for (spelling, expected) in [
        ("COUNTBLANK", 17_179_869_177.0_f64),
        ("COUNTA", 8.0),
        ("COUNT", 5.0),
    ] {
        s.reads.clear();
        s.visited = 0;
        let target = surface(Notation::Call, spelling);
        let mut given = Recorded::new(vec![whole_sheet.clone()]);
        let (result, bytes) = bytes_allocated_during(|| {
            black_box(invoke(
                target,
                &mut given,
                &mut EvalServices::with_cells(&fast, s),
                &mut FunctionCallScratch::new(),
            ))
        });
        assert_eq!(
            result
                .as_ref()
                .map(|value| value.as_number().map(f64::to_bits)),
            Ok(Some(expected.to_bits())),
            "{spelling} of the whole sheet: {result:?}"
        );
        assert_eq!((s.reads.as_slice(), s.visited), (&[whole][..], 8));
        assert!(bytes < SPARSE_BYTES, "{spelling} allocated {bytes} bytes");
        note!(
            "ACCEPT-NOTE: {CASE}: {spelling}(A1:XFD1048576) through calc_func::invoke = {expected} over {} cells; the reader was asked for 1 area and visited 8 cells; {bytes} bytes allocated",
            whole.cell_count()
        );
    }

    assert_workbook_sheet(AGGREGATE_WORKBOOK, AGGREGATE_WORKBOOK_PATH, &rows, s);
    aggregate_rows_note(&rows);
}

// ---------------------------------------------------------------------------------------------
// cfunc-m3.2: the rounding and integer functions (ABS, SIGN, INT, TRUNC, ROUND, ROUNDUP,
// ROUNDDOWN, MROUND, CEILING, FLOOR, MOD, QUOTIENT, EVEN and ODD) through calc_func::invoke on
// catalogue targets.
//
// Each oracle row runs one call and asserts a value derived here by hand, and says how, as the
// cfunc-m3.4 rows do. A `documented` row applies a rule or an example that Microsoft's page for
// the function states. An `unmeasured choice` row applies a rule no page settles; most of them
// tell candidate rules apart: a number read as the decimal it shows (2.675) or as its binary64
// (2.67499999999999982...), a quotient read exactly or rounded to binary64 first, a result beyond
// binary64's range, an odd integer above 2^53. A `refused` row ends in a fault, listed as
// unsupported in the per-ID manifest of `migration/ports/cfunc-m3.2.json`. No row has been
// compared with Excel.
//
// `tests/fixtures/cfunc_m3_2/oracle.xlsx`, written by the committed generator
// `tests/fixtures/make-fixtures.ps1`, holds every row's formula in column D of a sheet named after
// its case, in the order the case runs its rows, beside the cells the rows read, so that the owner
// can capture Excel's values; each case checks that its sheet holds exactly those formulas and
// cells. The cases do not wait for that capture: green shows the rules as written here, not
// agreement with Excel.

/// The rounding oracle workbook, as committed.
const ROUNDING_WORKBOOK: &[u8] = include_bytes!("fixtures/cfunc_m3_2/oracle.xlsx");
const ROUNDING_WORKBOOK_PATH: &str = "crates/calc-func/tests/fixtures/cfunc_m3_2/oracle.xlsx";
/// The bead's migration record, whose per-ID manifest the boundary case checks against the
/// catalogue.
const ROUNDING_RECORD: &str = include_str!("../../../migration/ports/cfunc-m3.2.json");

/// The family's entries: each spelling and the member number of its FunctionId in family 6.
const ROUNDING: [(&str, u32); 15] = [
    ("ABS", 1),
    ("SIGN", 2),
    ("INT", 3),
    ("TRUNC", 4),
    ("ROUND", 5),
    ("ROUNDUP", 6),
    ("ROUNDDOWN", 7),
    ("MROUND", 8),
    ("CEILING", 9),
    ("FLOOR", 10),
    ("MOD", 11),
    ("QUOTIENT", 12),
    ("EVEN", 13),
    ("ODD", 14),
    ("ECMA.CEILING", 15),
];

/// Runs a row of cfunc-m3.2: `rnd!(rows, sheet, "SPELLING", [args], expect, basis)`.
macro_rules! rnd {
    ($rows:expr, $sheet:expr, $spelling:expr, [$($arg:expr),* $(,)?], $expect:expr, $basis:expr) => {{
        let args = [$($arg),*];
        basis_row("cfunc-m3.2", $rows, $sheet, named($spelling), &args, $expect, $basis)
    }};
}

/// Prints how many rows a case ran and on what basis each expects its value.
fn rounding_rows_note(rows: &Rows) {
    let (choices, refused) = (rows.choices(), rows.refused());
    note!(
        "ACCEPT-NOTE: {}: {} rows through calc_func::invoke, each asserted against a value or a refusal derived here by hand and each with its formula on sheet {} of {ROUNDING_WORKBOOK_PATH}: {} from documented rules, {} unmeasured choices, {} refused; none is compared with Excel until the owner captures that workbook",
        rows.case,
        rows.oracle.len(),
        rows.sheet,
        rows.oracle.len() - choices - refused,
        choices,
        refused
    );
}

/// Checks that the committed rounding oracle workbook is its generator's output, on the route
/// where the generator runs.
fn assert_rounding_workbook_generated(case: &str) {
    #[cfg(all(windows, not(target_arch = "wasm32")))]
    {
        let generated = generated_fixtures();
        let names: Vec<&str> = generated.keys().map(String::as_str).collect();
        assert_eq!(
            names, GENERATED_WORKBOOKS,
            "the generator writes the oracle workbooks and nothing else"
        );
        let regenerated = &generated["cfunc_m3_2/oracle.xlsx"];
        assert!(
            regenerated.as_slice() == ROUNDING_WORKBOOK,
            "the committed rounding oracle workbook is not the generator's output (sha256 {} committed, {} generated)",
            sha256_hex(ROUNDING_WORKBOOK),
            sha256_hex(regenerated)
        );
        note!(
            "ACCEPT-NOTE: {case}: crates/calc-func/tests/fixtures/make-fixtures.ps1, run now under Windows PowerShell, wrote the committed rounding oracle workbook byte for byte ({} bytes)",
            regenerated.len()
        );
    }
    #[cfg(not(all(windows, not(target_arch = "wasm32"))))]
    {
        note!(
            "ACCEPT-NOTE: {case}: the generator runs under Windows PowerShell, so the check that the committed rounding oracle workbook is its output runs on the windows route; here the workbook is checked against the rows only"
        );
    }
}

// Rules the rows apply.
const HALF_AWAY: &str = "a half at num_digits 0 or below rounds away from zero, as the ROUND page's examples ROUND(2.15,1) = 2.2 and ROUND(-1.475,2) = -1.48 show for halves at 1 and 2 places; no example on the page shows a half at 0 or a negative num_digits";
const LEFT_OF_POINT: &str = "a negative num_digits rounds to the left of the decimal point";
const MOD_LIMIT: &str = "dc-j9k: MOD is #NUM! for a quotient of magnitude 1.1259E+12 or more (Excel 16.0.20430, bisected: MOD(1125899999999,1) is 0, MOD(1125900000000,1) is #NUM!)";
const MROUND_LIMITATION: &str = "the MROUND page's Known Limitations example: MROUND(6.05,0.1) returns 6.0 while MROUND(7.05,0.1) returns 7.1";
const NO_NEGATIVE_ZERO: &str = "the result is a zero, published as +0: a worksheet number carries no negative zero (the number rule of cfunc-m1.2)";
const NONNUMERIC: &str = "a nonnumeric argument is #VALUE!";
const FIRST_ERROR: &str = "the first argument's error is the result, before any domain rule, as the operators of cfunc-m1.2 read their operands";
const EXAMPLE: &str = "an example row of the function's page";
const SIGN_OF_DIVISOR: &str = "MOD(n, d) = n - d*INT(n/d): the result has the divisor's sign";
const LIFTED: &str = "an array argument, over which Excel lifts the function, is not modelled (array lifting is dc-m3u's)";

// ---------------------------------------------------------------------------------------------
// cfunc_m3_2__rounding_boundaries

/// The cells the boundary rows read: numbers whose shortest decimal needs more than 15
/// significant digits, so that they cannot be typed and are read as their binary64 values, beside
/// the INT page's example cell (A3). A6 is a normal number whose remainder by the typed
/// 5.70820472448858E-308 is not zero but too small for binary64.
fn rounding_boundaries_sheet() -> TestSheet {
    TestSheet::new(&[
        (
            "A1",
            "2.6749999999999994",
            CalcValue::number(2.674_999_999_999_999_4),
        ),
        (
            "A2",
            "0.30000000000000004",
            CalcValue::number(0.300_000_000_000_000_04),
        ),
        ("A3", "19.5", CalcValue::number(19.5)),
        (
            "A4",
            "1.0049999999999997",
            CalcValue::number(1.004_999_999_999_999_7),
        ),
        (
            "A5",
            "2.9999999999999996",
            CalcValue::number(2.999_999_999_999_999_6),
        ),
        (
            "A6",
            "1.141640944897716E-307",
            CalcValue::number(1.141_640_944_897_716e-307),
        ),
    ])
}

/// The rows of cfunc_m3_2__rounding_boundaries, run through calc_func::invoke on `s`, in the
/// order the rounding oracle workbook's sheet `rounding_boundaries` holds their formulas.
// 3.142 and 3.141 are the ROUNDUP and ROUNDDOWN pages' example results, not approximations of pi.
#[allow(clippy::approx_constant)]
fn rounding_boundaries_rows(s: &mut TestSheet) -> Rows {
    const CASE: &str = "cfunc_m3_2__rounding_boundaries";
    let mut rows = Rows::new(CASE, "rounding_boundaries", false);
    let r = &mut rows;
    use WorksheetErrorCode::{Div0, NA, Num, Ref, Value};

    // ROUND: the page's examples, half cases of both signs, signed zero and negative digits.
    rnd!(
        r,
        s,
        "ROUND",
        [num("2.15"), num("1")],
        N(2.2),
        Documented(EXAMPLE)
    );
    rnd!(
        r,
        s,
        "ROUND",
        [num("2.149"), num("1")],
        N(2.1),
        Documented(EXAMPLE)
    );
    rnd!(
        r,
        s,
        "ROUND",
        [num("-1.475"), num("2")],
        N(-1.48),
        Documented(EXAMPLE)
    );
    rnd!(
        r,
        s,
        "ROUND",
        [num("21.5"), num("-1")],
        N(20.0),
        Documented(EXAMPLE)
    );
    rnd!(
        r,
        s,
        "ROUND",
        [num("626.3"), num("-3")],
        N(1000.0),
        Documented(EXAMPLE)
    );
    rnd!(
        r,
        s,
        "ROUND",
        [num("1.98"), num("-1")],
        N(0.0),
        Documented(EXAMPLE)
    );
    rnd!(
        r,
        s,
        "ROUND",
        [num("-50.55"), num("-2")],
        N(-100.0),
        Documented(EXAMPLE)
    );
    rnd!(
        r,
        s,
        "ROUND",
        [num("2.5"), num("0")],
        N(3.0),
        Choice(HALF_AWAY)
    );
    rnd!(
        r,
        s,
        "ROUND",
        [num("-2.5"), num("0")],
        N(-3.0),
        Choice(HALF_AWAY)
    );
    rnd!(
        r,
        s,
        "ROUND",
        [num("0.5"), num("0")],
        N(1.0),
        Choice(HALF_AWAY)
    );
    rnd!(
        r,
        s,
        "ROUND",
        [num("-0.5"), num("0")],
        N(-1.0),
        Choice(HALF_AWAY)
    );
    rnd!(
        r,
        s,
        "ROUND",
        [num("-0.4"), num("0")],
        N(0.0),
        Choice(NO_NEGATIVE_ZERO)
    );
    rnd!(
        r,
        s,
        "ROUND",
        [num("-0.0001"), num("2")],
        N(0.0),
        Choice(NO_NEGATIVE_ZERO)
    );
    rnd!(
        r,
        s,
        "ROUND",
        [num("25"), num("-1")],
        N(30.0),
        Choice(HALF_AWAY)
    );
    rnd!(
        r,
        s,
        "ROUND",
        [num("-15"), num("-1")],
        N(-20.0),
        Choice(HALF_AWAY)
    );
    rnd!(
        r,
        s,
        "ROUND",
        [num("1250"), num("-2")],
        N(1300.0),
        Choice(HALF_AWAY)
    );
    rnd!(
        r,
        s,
        "ROUND",
        [num("1234"), num("-2")],
        N(1200.0),
        Documented(LEFT_OF_POINT)
    );
    rnd!(
        r,
        s,
        "ROUND",
        [num("0.5"), num("-1")],
        N(0.0),
        Documented(LEFT_OF_POINT)
    );

    // The reading: a number is the decimal it shows when that has at most 15 significant digits,
    // and its binary64 value otherwise. The page's example ROUND(2.15,1) = 2.2 above already rules
    // out rounding the exact binary64 (2.14999999999999991... gives 2.1). These rows tell the
    // reading from that binary rule and from the 15-digit rule (every number read at 15
    // significant digits).
    rnd!(
        r,
        s,
        "ROUND",
        [num("2.675"), num("2")],
        N(2.68),
        Choice(
            "2.675 is read as the decimal it shows, a half; its binary64 2.67499999999999982... lies below the half, so the binary rule gives 2.67"
        )
    );
    rnd!(
        r,
        s,
        "ROUND",
        [num("-2.675"), num("2")],
        N(-2.68),
        Choice(
            "-2.675 is read as the decimal it shows, a half rounded away from zero; the binary rule gives -2.67"
        )
    );
    rnd!(
        r,
        s,
        "ROUND",
        [num("1.005"), num("2")],
        N(1.01),
        Choice(
            "1.005 is read as the decimal it shows; its binary64 1.00499999999999989... gives 1.00 by the binary rule"
        )
    );
    rnd!(
        r,
        s,
        "ROUND",
        [cell(s, "A1"), num("2")],
        N(2.68),
        Measured(
            "dc-bd5: ROUND reads its number rounded to 15 significant digits, 2.67500000000000 (Excel 16.0.20430, cfunc_m3_2's capture)"
        )
    );
    rnd!(
        r,
        s,
        "ROUND",
        [cell(s, "A4"), num("2")],
        N(1.01),
        Measured(
            "dc-bd5: the 15-digit reading 1.00500000000000 rounds to 1.01 (Excel 16.0.20430, cfunc_m3_2's capture)"
        )
    );
    rnd!(
        r,
        s,
        "ROUNDUP",
        [cell(s, "A2"), num("1")],
        N(0.3),
        Measured(
            "dc-bd5: the 15-digit reading 0.300000000000000 has no digit to round up (Excel 16.0.20430, cfunc_m3_2's capture)"
        )
    );
    rnd!(
        r,
        s,
        "ROUNDDOWN",
        [num("0.57"), num("2")],
        N(0.57),
        Choice(
            "0.57 is read as the decimal it shows; its binary64 0.56999999999999995... gives 0.56 by the binary rule"
        )
    );
    rnd!(
        r,
        s,
        "TRUNC",
        [num("0.57"), num("2")],
        N(0.57),
        Choice("TRUNC reads as ROUNDDOWN does; the binary rule gives 0.56")
    );
    rnd!(
        r,
        s,
        "TRUNC",
        [cell(s, "A5")],
        N(3.0),
        Measured(
            "dc-bd5: TRUNC reads 2.9999999999999996 to 15 significant digits, 3 (Excel 16.0.20430, cfunc_m3_2's capture)"
        )
    );
    rnd!(
        r,
        s,
        "INT",
        [cell(s, "A5")],
        N(3.0),
        Measured(
            "dc-bd5: INT reads a number below 1E15 to 15 significant digits, 3.00000000000000 (Excel 16.0.20430, cfunc_m3_2's capture; INT(2.9999999999999982) is 3 too, 16.0.20527)"
        )
    );

    // ROUNDUP and ROUNDDOWN: the pages' examples.
    rnd!(
        r,
        s,
        "ROUNDUP",
        [num("3.2"), num("0")],
        N(4.0),
        Documented(EXAMPLE)
    );
    rnd!(
        r,
        s,
        "ROUNDUP",
        [num("76.9"), num("0")],
        N(77.0),
        Documented(EXAMPLE)
    );
    rnd!(
        r,
        s,
        "ROUNDUP",
        [num("3.14159"), num("3")],
        N(3.142),
        Documented(EXAMPLE)
    );
    rnd!(
        r,
        s,
        "ROUNDUP",
        [num("-3.14159"), num("1")],
        N(-3.2),
        Documented(EXAMPLE)
    );
    rnd!(
        r,
        s,
        "ROUNDUP",
        [num("31415.92654"), num("-2")],
        N(31500.0),
        Documented(EXAMPLE)
    );
    rnd!(
        r,
        s,
        "ROUNDUP",
        [num("-0.1"), num("0")],
        N(-1.0),
        Documented(
            "ROUNDUP rounds away from zero, as its example ROUNDUP(-3.14159,1) = -3.2 shows"
        )
    );
    rnd!(
        r,
        s,
        "ROUNDDOWN",
        [num("3.2"), num("0")],
        N(3.0),
        Documented(EXAMPLE)
    );
    rnd!(
        r,
        s,
        "ROUNDDOWN",
        [num("76.9"), num("0")],
        N(76.0),
        Documented(EXAMPLE)
    );
    rnd!(
        r,
        s,
        "ROUNDDOWN",
        [num("3.14159"), num("3")],
        N(3.141),
        Documented(EXAMPLE)
    );
    rnd!(
        r,
        s,
        "ROUNDDOWN",
        [num("-3.14159"), num("1")],
        N(-3.1),
        Documented(EXAMPLE)
    );
    rnd!(
        r,
        s,
        "ROUNDDOWN",
        [num("31415.92654"), num("-2")],
        N(31400.0),
        Documented(EXAMPLE)
    );
    rnd!(
        r,
        s,
        "ROUNDDOWN",
        [num("-0.5"), num("0")],
        N(0.0),
        Choice(NO_NEGATIVE_ZERO)
    );

    // num_digits: only its integer part counts, and a position beyond every digit is reached
    // without an integer type.
    rnd!(
        r,
        s,
        "ROUND",
        [num("1.2345"), num("2.9")],
        N(1.23),
        Choice("num_digits counts by its integer part, 2.9 as 2; the page is silent")
    );
    rnd!(
        r,
        s,
        "ROUND",
        [num("1234.5"), num("-1.9")],
        N(1230.0),
        Choice("num_digits counts by its integer part, -1.9 as -1; the page is silent")
    );
    rnd!(
        r,
        s,
        "ROUND",
        [num("1.5"), num("400")],
        N(1.5),
        Choice("no digit lies 400 places right of the point, so nothing is rounded")
    );
    rnd!(
        r,
        s,
        "ROUND",
        [num("1.5"), num("-400")],
        N(0.0),
        Choice("the nearest multiple of 10^400 is 0")
    );
    rnd!(
        r,
        s,
        "ROUNDUP",
        [num("1.5"), num("-400")],
        E(Num),
        Choice("rounded up to 10^400, beyond binary64's range: #NUM!, the number rule's overflow")
    );

    // TRUNC and INT: the pages' examples.
    rnd!(r, s, "TRUNC", [num("8.9")], N(8.0), Documented(EXAMPLE));
    rnd!(r, s, "TRUNC", [num("-8.9")], N(-8.0), Documented(EXAMPLE));
    rnd!(r, s, "TRUNC", [num("0.45")], N(0.0), Documented(EXAMPLE));
    rnd!(
        r,
        s,
        "TRUNC",
        [num("-4.3")],
        N(-4.0),
        Documented("TRUNC removes the fractional part: TRUNC(-4.3) is -4 where INT(-4.3) is -5")
    );
    rnd!(
        r,
        s,
        "INT",
        [num("-4.3")],
        N(-5.0),
        Documented("INT rounds down to the lower integer: INT(-4.3) is -5")
    );
    rnd!(r, s, "INT", [num("8.9")], N(8.0), Documented(EXAMPLE));
    rnd!(r, s, "INT", [num("-8.9")], N(-9.0), Documented(EXAMPLE));
    rnd!(
        r,
        s,
        "INT",
        [cell(s, "A3")],
        N(19.0),
        Documented(
            "INT rounds down to the nearest integer: the page's example =A2-INT(A2) is 0.5 for A2 = 19.5, so INT(19.5) is 19"
        )
    );
    rnd!(
        r,
        s,
        "TRUNC",
        [num("-0.5")],
        N(0.0),
        Choice(NO_NEGATIVE_ZERO)
    );
    rnd!(
        r,
        s,
        "TRUNC",
        [num("8.9"), omitted()],
        N(8.0),
        Choice("an omitted num_digits is 0, as an omitted argument is the number 0")
    );

    // MROUND: the count of multiples comes from the binary64 quotient, an unmeasured choice that
    // agrees with both outcomes of the page's Known Limitations example; the count times the
    // multiple's exact decimal reading is converted once, an unmeasured choice.
    rnd!(
        r,
        s,
        "MROUND",
        [num("10"), num("3")],
        N(9.0),
        Documented(EXAMPLE)
    );
    rnd!(
        r,
        s,
        "MROUND",
        [num("-10"), num("-3")],
        N(-9.0),
        Documented(EXAMPLE)
    );
    rnd!(
        r,
        s,
        "MROUND",
        [num("1.3"), num("0.2")],
        N(1.4000000000000001),
        Measured(
            "dc-j9k: the binary64 quotient 6.5 rounds away from zero to 7 multiples, and the binary64 product 7*0.2 is 1.4000000000000001, as Excel 16.0.20430 gives MROUND(1.3,0.2) (the page's 1.4 is shown to 15 digits)"
        )
    );
    rnd!(
        r,
        s,
        "MROUND",
        [num("5"), num("-2")],
        E(Num),
        Documented(EXAMPLE)
    );
    rnd!(
        r,
        s,
        "MROUND",
        [num("-5"), num("2")],
        E(Num),
        Documented("number and multiple of different signs are #NUM!")
    );
    rnd!(
        r,
        s,
        "MROUND",
        [num("7.5"), num("5")],
        N(10.0),
        Documented("a remainder of at least half the multiple rounds up, away from zero")
    );
    rnd!(
        r,
        s,
        "MROUND",
        [num("6.05"), num("0.1")],
        N(6.0),
        Documented(MROUND_LIMITATION)
    );
    rnd!(
        r,
        s,
        "MROUND",
        [num("7.05"), num("0.1")],
        N(7.1000000000000005),
        Measured(
            "dc-j9k: the binary64 quotient 70.5 rounds away from zero to 71 multiples, and the binary64 product 71*0.1 is 7.1000000000000005, as Excel 16.0.20430 gives MROUND(7.05,0.1) (the page's 7.1 is shown to 15 digits)"
        )
    );
    rnd!(
        r,
        s,
        "MROUND",
        [num("-6.05"), num("-0.1")],
        N(-6.0),
        Choice(
            "as the page's example MROUND(6.05,0.1) = 6.0, with both signs negative: the binary64 quotient is again 60.49999999999999, rounded to 60 multiples; the exact decimal readings give the half 60.5 and so -6.1"
        )
    );
    rnd!(
        r,
        s,
        "MROUND",
        [num("5"), num("0")],
        N(0.0),
        Choice("a zero multiple gives 0; the page is silent")
    );

    // CEILING.
    rnd!(
        r,
        s,
        "CEILING",
        [num("2.5"), num("1")],
        N(3.0),
        Documented(EXAMPLE)
    );
    rnd!(
        r,
        s,
        "CEILING",
        [num("-2.5"), num("-2")],
        N(-4.0),
        Documented(EXAMPLE)
    );
    rnd!(
        r,
        s,
        "CEILING",
        [num("-2.5"), num("2")],
        N(-2.0),
        Documented(EXAMPLE)
    );
    rnd!(
        r,
        s,
        "CEILING",
        [num("1.5"), num("0.1")],
        N(1.5),
        Documented(EXAMPLE)
    );
    rnd!(
        r,
        s,
        "CEILING",
        [num("0.234"), num("0.01")],
        N(0.24),
        Documented(EXAMPLE)
    );
    rnd!(
        r,
        s,
        "CEILING",
        [num("4.3"), num("2")],
        N(6.0),
        Documented("a value is rounded up, away from zero, to a multiple of the significance")
    );
    rnd!(
        r,
        s,
        "CEILING",
        [num("-4.3"), num("2")],
        N(-4.0),
        Documented("a negative number with a positive significance is rounded up, toward zero")
    );
    rnd!(
        r,
        s,
        "CEILING",
        [num("-4.3"), num("-2")],
        N(-6.0),
        Documented(
            "a negative number with a negative significance is rounded down, away from zero"
        )
    );
    rnd!(
        r,
        s,
        "CEILING",
        [num("4.3"), num("-2")],
        E(Num),
        Choice(
            "a positive number with a negative significance is #NUM!, as the FLOOR page states for FLOOR and the legacy CEILING test expects; the CEILING page names no result for it: its remarks cover a negative number with either significance and say only that a value is rounded up when adjusted away from zero"
        )
    );
    rnd!(
        r,
        s,
        "CEILING",
        [num("5"), num("0")],
        N(0.0),
        Choice("a zero significance gives 0, as the legacy test expects; the page is silent")
    );
    rnd!(
        r,
        s,
        "CEILING",
        [num("0.3"), num("0.1")],
        N(0.300_000_000_000_000_04),
        Choice(
            "the binary64 order: the quotient 0.3/0.1 rounds to 2.9999999999999996, whose integer above is 3, and 0.1*3 rounds to 0.30000000000000004; read as typed decimals 0.3 is 3 multiples of 0.1, an exact multiple the page says is not rounded, so 0.3"
        )
    );
    rnd!(
        r,
        s,
        "CEILING",
        [num("-0.5"), num("1")],
        N(0.0),
        Choice(NO_NEGATIVE_ZERO)
    );

    // FLOOR.
    rnd!(
        r,
        s,
        "FLOOR",
        [num("3.7"), num("2")],
        N(2.0),
        Documented(EXAMPLE)
    );
    rnd!(
        r,
        s,
        "FLOOR",
        [num("-2.5"), num("-2")],
        N(-2.0),
        Documented(EXAMPLE)
    );
    rnd!(
        r,
        s,
        "FLOOR",
        [num("2.5"), num("-2")],
        E(Num),
        Documented(EXAMPLE)
    );
    rnd!(
        r,
        s,
        "FLOOR",
        [num("1.58"), num("0.1")],
        N(1.5),
        Documented(EXAMPLE)
    );
    rnd!(
        r,
        s,
        "FLOOR",
        [num("0.234"), num("0.01")],
        N(0.23),
        Documented(EXAMPLE)
    );
    rnd!(
        r,
        s,
        "FLOOR",
        [num("4.3"), num("2")],
        N(4.0),
        Documented("a positive number is rounded down, toward zero")
    );
    rnd!(
        r,
        s,
        "FLOOR",
        [num("-4.3"), num("2")],
        N(-6.0),
        Documented("a negative number is rounded down, away from zero")
    );
    rnd!(
        r,
        s,
        "FLOOR",
        [num("-4.3"), num("-2")],
        N(-4.0),
        Documented("both negative, toward zero, as the page's example FLOOR(-2.5,-2) = -2 shows")
    );
    rnd!(
        r,
        s,
        "FLOOR",
        [num("4.3"), num("-2")],
        E(Num),
        Documented("a positive number with a negative significance is #NUM!")
    );
    rnd!(
        r,
        s,
        "FLOOR",
        [num("-2.5"), num("2")],
        N(-4.0),
        Documented("a negative number is rounded down, away from zero")
    );
    rnd!(
        r,
        s,
        "FLOOR",
        [num("5"), num("0")],
        E(Div0),
        Choice("a zero significance is #DIV/0!, as the legacy test expects; the page is silent")
    );
    rnd!(
        r,
        s,
        "FLOOR",
        [num("0.15"), num("0.05")],
        N(0.15000000000000002),
        Measured(
            "dc-j9k: the quotient 0.15/0.05, 2.9999999999999996, is counted at 15 significant digits, 3, and 0.05*3 is 0.15000000000000002, as Excel 16.0.20430 gives FLOOR(0.15,0.05)"
        )
    );
    rnd!(
        r,
        s,
        "FLOOR",
        [num("0.3"), num("0.1")],
        N(0.30000000000000004),
        Measured(
            "dc-j9k: the quotient is counted at 15 significant digits, as Excel 16.0.20430 counts it (the catalogue oracle's sheet dcj9k_math)"
        )
    );
    rnd!(
        r,
        s,
        "FLOOR",
        [cell(s, "A2"), num("0.1")],
        N(0.300_000_000_000_000_04),
        Choice(
            "the binary64 order: A2's binary64 0.30000000000000004 divided by 0.1 rounds to 3.0000000000000004, whose integer below is 3, and 0.1*3 rounds to 0.30000000000000004; A2 read exactly is just above 3 multiples of 0.1 read as typed, so 0.3"
        )
    );

    // MOD.
    rnd!(
        r,
        s,
        "MOD",
        [num("3"), num("2")],
        N(1.0),
        Documented(EXAMPLE)
    );
    rnd!(
        r,
        s,
        "MOD",
        [num("-3"), num("2")],
        N(1.0),
        Documented(EXAMPLE)
    );
    rnd!(
        r,
        s,
        "MOD",
        [num("3"), num("-2")],
        N(-1.0),
        Documented(EXAMPLE)
    );
    rnd!(
        r,
        s,
        "MOD",
        [num("-3"), num("-2")],
        N(-1.0),
        Documented(EXAMPLE)
    );
    rnd!(
        r,
        s,
        "MOD",
        [num("3"), num("0")],
        E(Div0),
        Documented("a divisor of 0 is #DIV/0!")
    );
    rnd!(
        r,
        s,
        "MOD",
        [num("-7"), num("2.5")],
        N(0.5),
        Documented(SIGN_OF_DIVISOR)
    );
    rnd!(
        r,
        s,
        "MOD",
        [num("0.3"), num("0.1")],
        N(0.09999999999999998),
        Measured(
            "dc-j9k: the exact binary64 remainder, as Excel 16.0.20430 gives it (the catalogue oracle's sheet dcj9k_math)"
        )
    );
    rnd!(
        r,
        s,
        "MOD",
        [num("1"), num("0.1")],
        N(0.09999999999999995),
        Measured(
            "dc-j9k: the exact binary64 remainder, as Excel 16.0.20430 gives it (the catalogue oracle's sheet dcj9k_math)"
        )
    );

    // QUOTIENT.
    rnd!(
        r,
        s,
        "QUOTIENT",
        [num("5"), num("2")],
        N(2.0),
        Documented(EXAMPLE)
    );
    rnd!(
        r,
        s,
        "QUOTIENT",
        [num("4.5"), num("3.1")],
        N(1.0),
        Documented(EXAMPLE)
    );
    rnd!(
        r,
        s,
        "QUOTIENT",
        [num("-10"), num("3")],
        N(-3.0),
        Documented(EXAMPLE)
    );
    rnd!(
        r,
        s,
        "QUOTIENT",
        [txt("a"), num("2")],
        E(Value),
        Documented(NONNUMERIC)
    );
    rnd!(
        r,
        s,
        "QUOTIENT",
        [num("1"), num("0")],
        E(Div0),
        Choice("a zero denominator is #DIV/0!; the page is silent")
    );
    rnd!(
        r,
        s,
        "QUOTIENT",
        [num("0.3"), num("0.1")],
        N(2.0),
        Measured(
            "dc-bd5: the binary64 quotient 2.9999999999999996, toward zero (Excel 16.0.20430, cfunc_m3_2's capture)"
        )
    );
    rnd!(
        r,
        s,
        "QUOTIENT",
        [num("-1"), num("3")],
        N(0.0),
        Choice(NO_NEGATIVE_ZERO)
    );

    // EVEN and ODD.
    rnd!(r, s, "EVEN", [num("1.5")], N(2.0), Documented(EXAMPLE));
    rnd!(r, s, "EVEN", [num("3")], N(4.0), Documented(EXAMPLE));
    rnd!(r, s, "EVEN", [num("2")], N(2.0), Documented(EXAMPLE));
    rnd!(r, s, "EVEN", [num("-1")], N(-2.0), Documented(EXAMPLE));
    rnd!(
        r,
        s,
        "EVEN",
        [num("-0.5")],
        N(-2.0),
        Documented("a value is rounded up when adjusted away from zero")
    );
    rnd!(
        r,
        s,
        "EVEN",
        [num("-1.5")],
        N(-2.0),
        Documented("a value is rounded up when adjusted away from zero")
    );
    rnd!(
        r,
        s,
        "EVEN",
        [num("0")],
        N(0.0),
        Documented("an even integer is not rounded")
    );
    rnd!(r, s, "EVEN", [txt("a")], E(Value), Documented(NONNUMERIC));
    rnd!(r, s, "ODD", [num("1.5")], N(3.0), Documented(EXAMPLE));
    rnd!(r, s, "ODD", [num("3")], N(3.0), Documented(EXAMPLE));
    rnd!(r, s, "ODD", [num("2")], N(3.0), Documented(EXAMPLE));
    rnd!(r, s, "ODD", [num("-1")], N(-1.0), Documented(EXAMPLE));
    rnd!(r, s, "ODD", [num("-2")], N(-3.0), Documented(EXAMPLE));
    rnd!(
        r,
        s,
        "ODD",
        [num("-1.5")],
        N(-3.0),
        Documented("a value is rounded up when adjusted away from zero")
    );
    rnd!(
        r,
        s,
        "ODD",
        [num("0")],
        N(1.0),
        Choice("0 rounds to 1, as the legacy kernel does; the page is silent on zero")
    );
    rnd!(r, s, "ODD", [txt("a")], E(Value), Documented(NONNUMERIC));

    // ABS and SIGN.
    rnd!(r, s, "ABS", [num("2")], N(2.0), Documented(EXAMPLE));
    rnd!(r, s, "ABS", [num("-2")], N(2.0), Documented(EXAMPLE));
    rnd!(
        r,
        s,
        "ABS",
        [num("-2.5")],
        N(2.5),
        Documented("ABS is the number without its sign")
    );
    rnd!(
        r,
        s,
        "ABS",
        [txt(" -2 ")],
        N(2.0),
        Choice(
            "number text typed as an argument is read through worksheet number coercion, as cfunc-m1.2 reads it and the legacy test expects"
        )
    );
    rnd!(
        r,
        s,
        "ABS",
        [txt("asd")],
        E(Value),
        Choice("typed text that does not read as a number is #VALUE!, as the legacy test expects")
    );
    rnd!(
        r,
        s,
        "ABS",
        [logical(true)],
        N(1.0),
        Choice(
            "a logical typed as an argument is a number, TRUE as 1, as cfunc-m1.2's coercion reads it"
        )
    );
    rnd!(r, s, "ABS", [err(Div0)], E(Div0), Choice(FIRST_ERROR));
    rnd!(r, s, "SIGN", [num("10")], N(1.0), Documented(EXAMPLE));
    rnd!(
        r,
        s,
        "SIGN",
        [num("-0.00001")],
        N(-1.0),
        Documented(EXAMPLE)
    );
    rnd!(
        r,
        s,
        "SIGN",
        [num("0")],
        N(0.0),
        Documented("SIGN returns zero (0) if the number is 0")
    );

    // Argument order, coercion and arrays.
    rnd!(
        r,
        s,
        "ROUND",
        [err(NA), err(Div0)],
        E(NA),
        Choice(FIRST_ERROR)
    );
    rnd!(r, s, "MOD", [err(NA), num("0")], E(NA), Choice(FIRST_ERROR));
    rnd!(
        r,
        s,
        "FLOOR",
        [num("1"), err(Ref)],
        E(Ref),
        Choice(FIRST_ERROR)
    );
    rnd!(
        r,
        s,
        "CEILING",
        [txt("2.5"), logical(true)],
        N(3.0),
        Choice(
            "number text and a logical typed as arguments are read as numbers, as cfunc-m1.2's coercion reads them"
        )
    );
    rnd!(
        r,
        s,
        "CEILING",
        [txt("a"), num("1")],
        E(Value),
        Documented(NONNUMERIC)
    );
    rnd!(
        r,
        s,
        "ABS",
        [array_arg(
            "{-1,2}",
            vec![vec![CalcValue::number(-1.0), CalcValue::number(2.0)]]
        )],
        Refused(Some(0)),
        Unsupported(LIFTED)
    );
    rnd!(
        r,
        s,
        "ROUND",
        [
            num("1.5"),
            array_arg(
                "{0,1}",
                vec![vec![CalcValue::number(0.0), CalcValue::number(1.0)]]
            )
        ],
        Refused(Some(1)),
        Unsupported(LIFTED)
    );
    rnd!(
        r,
        s,
        "ROUNDDOWN",
        [num("2.5E-308"), num("308")],
        N(0.0),
        Measured(
            "dc-bd5: a result below the normal range is 0 (Excel 16.0.20430, cfunc_m3_2's capture)"
        )
    );
    rnd!(
        r,
        s,
        "MOD",
        [cell(s, "A6"), num("5.70820472448858E-308")],
        N(0.0),
        Measured(
            "dc-j9k: the exact binary64 remainder of A6 by 5.70820472448858E-308 is 0, as Excel 16.0.20430 gives it with A6 set to 1.141640944897716E-307"
        )
    );
    rows
}

#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test)]
fn cfunc_m3_2__rounding_boundaries() {
    const CASE: &str = "cfunc_m3_2__rounding_boundaries";
    let fast = NumericMode::Fast;
    let exact = exact_mode();
    let mut sheet = rounding_boundaries_sheet();
    note!("ACCEPT-NOTE: {CASE}: the sheet holds {}", sheet.described());
    let rows = rounding_boundaries_rows(&mut sheet);

    // Metadata and dispatch come from the one generated catalogue: family 6 holds exactly these
    // entries, each eager, with single-value parameters, Excel's arity and its signature help.
    let family: BTreeSet<(u32, String)> = catalogue()
        .targets()
        .filter(|target| target.id().raw() >> 16 == 6)
        .map(|target| (target.id().raw(), target.spelling().to_owned()))
        .collect();
    let expected: BTreeSet<(u32, String)> = ROUNDING
        .iter()
        .map(|(spelling, member)| ((6 << 16) | member, (*spelling).to_owned()))
        .collect();
    assert_eq!(family, expected, "the rounding family in the catalogue");
    for (spelling, _) in ROUNDING {
        let target = surface(Notation::Call, spelling);
        assert_eq!(
            target.argument_demand(),
            ArgumentDemand::Eager,
            "{spelling}"
        );
        let arity = target.arity();
        let shape = match spelling {
            "ABS" | "SIGN" | "INT" | "EVEN" | "ODD" => (1, 1),
            "TRUNC" => (1, 2),
            _ => (2, 2),
        };
        assert_eq!((arity.min, arity.max), shape, "{spelling}");
        for index in 0..arity.max {
            assert_eq!(
                target.parameter_class(index),
                Some(ParameterClass::Value),
                "{spelling} argument {index}"
            );
        }
    }
    for (spelling, display) in [
        ("ABS", "ABS(number)"),
        ("TRUNC", "TRUNC(number, [num_digits])"),
        ("ROUND", "ROUND(number, num_digits)"),
        ("MROUND", "MROUND(number, multiple)"),
        ("CEILING", "CEILING(number, significance)"),
        ("MOD", "MOD(number, divisor)"),
        ("QUOTIENT", "QUOTIENT(numerator, denominator)"),
    ] {
        assert_eq!(
            surface(Notation::Call, spelling).signature_help().display,
            display
        );
    }
    let manifest = manifest_ids(ROUNDING_RECORD);
    let listed: BTreeSet<(String, String)> = ROUNDING
        .iter()
        .map(|(spelling, member)| (format!("6.{member}"), (*spelling).to_owned()))
        .collect();
    assert_eq!(
        manifest, listed,
        "the per-ID manifest of migration/ports/cfunc-m3.2.json against the catalogue"
    );
    note!(
        "ACCEPT-NOTE: {CASE}: the catalogue's family 6 holds exactly the {} entries {:?}, each eager with single-value parameters; TRUNC takes 1 or 2 arguments, ABS, SIGN, INT, EVEN and ODD 1, the others 2; the per-ID manifest of migration/ports/cfunc-m3.2.json lists the same IDs and spellings",
        family.len(),
        ROUNDING.map(|(spelling, _)| spelling)
    );

    // A value parameter never receives a reference: the caller dereferences it, and the call
    // interface refuses one at admission, before the kernel runs.
    for (spelling, index) in [("ABS", 0), ("ROUND", 1), ("MOD", 0)] {
        let target = surface(Notation::Call, spelling);
        let mut values = vec![CalcValue::number(1.0); target.arity().min];
        values[index] = a1_reference();
        let mut given = Recorded::new(values);
        assert_eq!(
            run(target, &mut given, &fast),
            Err(FunctionFault::ReferenceArgument {
                id: target.id(),
                index
            }),
            "{spelling} with a reference at argument {index}"
        );
    }

    // Through the call interface alone (no formula or cell holds -0): a negative zero argument
    // reads as 0, and no result is a negative zero.
    let negative_zero: [(&str, Vec<f64>, Result<f64, WorksheetErrorCode>); 20] = [
        ("ABS", vec![-0.0], Ok(0.0)),
        ("SIGN", vec![-0.0], Ok(0.0)),
        ("INT", vec![-0.0], Ok(0.0)),
        ("TRUNC", vec![-0.0], Ok(0.0)),
        ("ROUND", vec![-0.0, 2.0], Ok(0.0)),
        ("ROUNDUP", vec![-0.0, 0.0], Ok(0.0)),
        ("ROUNDDOWN", vec![-0.0, 0.0], Ok(0.0)),
        ("MROUND", vec![-0.0, 2.0], Ok(0.0)),
        ("CEILING", vec![-0.0, 1.0], Ok(0.0)),
        ("FLOOR", vec![-0.0, 1.0], Ok(0.0)),
        ("MOD", vec![-0.0, 3.0], Ok(0.0)),
        ("QUOTIENT", vec![-0.0, 3.0], Ok(0.0)),
        ("EVEN", vec![-0.0], Ok(0.0)),
        ("ODD", vec![-0.0], Ok(1.0)),
        ("ROUND", vec![2.5, -0.0], Ok(3.0)),
        ("CEILING", vec![1.0, -0.0], Ok(0.0)),
        ("MROUND", vec![1.0, -0.0], Ok(0.0)),
        ("FLOOR", vec![1.0, -0.0], Err(WorksheetErrorCode::Div0)),
        ("MOD", vec![1.0, -0.0], Err(WorksheetErrorCode::Div0)),
        ("QUOTIENT", vec![1.0, -0.0], Err(WorksheetErrorCode::Div0)),
    ];
    for (spelling, numbers, expected) in &negative_zero {
        let target = surface(Notation::Call, spelling);
        let mut given = Recorded::new(numbers.iter().map(|&n| CalcValue::number(n)).collect());
        let result = run(target, &mut given, &fast);
        let holds = match expected {
            Ok(n) => result
                .as_ref()
                .is_ok_and(|value| value.as_number().map(f64::to_bits) == Some(n.to_bits())),
            Err(code) => result
                .as_ref()
                .is_ok_and(|value| value.as_error() == Some(*code)),
        };
        assert!(
            holds,
            "{spelling}{numbers:?}: expected {expected:?}, got {result:?}"
        );
    }
    note!(
        "ACCEPT-NOTE: {CASE}: {} calls with a -0 argument through the call interface alone gave +0 (bits 0x0000000000000000) or the value of a 0 argument, never -0",
        negative_zero.len()
    );

    // NumericMode::Exact: a call that reaches no arithmetic site computes the value of its Fast
    // row; one that converts number text or an exact result, divides (MROUND, CEILING and FLOOR),
    // or steps ODD or EVEN on, is refused.
    let mut exact_computed = 0;
    for (spelling, args) in [
        ("ABS", vec![num("-2")]),
        ("SIGN", vec![num("-0.00001")]),
        ("INT", vec![num("-8.9")]),
        ("EVEN", vec![num("1.5")]),
        ("EVEN", vec![num("2")]),
        ("ODD", vec![num("3")]),
        ("ODD", vec![num("0")]),
        ("ROUND", vec![num("1.5"), num("400")]),
        ("MOD", vec![num("3"), num("0")]),
        ("MROUND", vec![num("5"), num("-2")]),
        ("FLOOR", vec![num("2.5"), num("-2")]),
        ("CEILING", vec![num("5"), num("0")]),
        ("QUOTIENT", vec![num("1"), num("0")]),
    ] {
        let target = surface(Notation::Call, spelling);
        let texts: Vec<&str> = args.iter().map(|a| a.excel.as_str()).collect();
        let formula = target.oracle_formula(&texts).expect("a formula");
        let under_fast = row_value(&rows, &formula);
        let under_exact = run(
            target,
            &mut Recorded::new(args.iter().map(|a| a.value.clone()).collect()),
            &exact,
        );
        assert!(
            under_exact
                .as_ref()
                .is_ok_and(|value| identical(value, under_fast)),
            "{formula} under Exact gives {under_exact:?}, under Fast {under_fast:?}"
        );
        exact_computed += 1;
    }
    let mut exact_refused = 0;
    for (spelling, args) in [
        ("ROUND", vec![num("2.675"), num("2")]),
        ("ROUNDUP", vec![num("3.2"), num("0")]),
        ("ROUNDDOWN", vec![num("3.2"), num("0")]),
        ("TRUNC", vec![num("8.9")]),
        ("MROUND", vec![num("10"), num("3")]),
        ("CEILING", vec![num("2.5"), num("1")]),
        ("FLOOR", vec![num("3.7"), num("2")]),
        ("MOD", vec![num("3"), num("2")]),
        ("QUOTIENT", vec![num("5"), num("2")]),
        ("EVEN", vec![num("3")]),
        ("ODD", vec![num("2")]),
        // Number text is converted to its nearest binary64 at an arithmetic site, so even the
        // functions that act on the binary64 itself refuse it under Exact.
        ("ABS", vec![txt(" -2 ")]),
        ("INT", vec![txt("8.9")]),
        ("SIGN", vec![txt("-0.00001")]),
    ] {
        let target = surface(Notation::Call, spelling);
        let result = run(
            target,
            &mut Recorded::new(args.iter().map(|a| a.value.clone()).collect()),
            &exact,
        );
        assert_eq!(
            result,
            Err(FunctionFault::ExactUnavailable { id: target.id() }),
            "{spelling} under Exact"
        );
        exact_refused += 1;
    }
    note!(
        "ACCEPT-NOTE: {CASE}: under NumericMode::Exact {exact_computed} calls that reach no arithmetic site (ABS, SIGN, INT, EVEN and ODD on a number with no step, a ROUND with no digit to drop, and the domain errors) computed the value of their Fast oracle row, and {exact_refused} calls that convert number text or an exact result, divide (MROUND, CEILING and FLOOR), or step EVEN or ODD on, were refused with ExactUnavailable"
    );

    // The oracle workbook: this case's sheet holds exactly its rows and cells, and the committed
    // workbook is its generator's output.
    assert_workbook_sheet(ROUNDING_WORKBOOK, ROUNDING_WORKBOOK_PATH, &rows, &sheet);
    assert_rounding_workbook_generated(CASE);
    rounding_rows_note(&rows);
}

// ---------------------------------------------------------------------------------------------
// cfunc_m3_2__large_integers

/// The cells the large-integer rows read: numbers around 2^53, 2^52 and 1E15 with a half, 2^60,
/// 2^63, 2^64 and the largest binary64 of both signs. Each but A12 (2^53-2, which has 15
/// significant digits) needs more than 15 significant digits, so it cannot be typed and is read
/// as its binary64 value; a row whose value would differ under the 15-digit reading (each such
/// cell rounded to 15 significant digits first) is an unmeasured choice that names that reading.
fn large_integers_sheet() -> TestSheet {
    TestSheet::new(&[
        ("A1", "2^53", CalcValue::number(9_007_199_254_740_992.0)),
        ("A2", "2^53-1", CalcValue::number(9_007_199_254_740_991.0)),
        ("A3", "2^53+2", CalcValue::number(9_007_199_254_740_994.0)),
        ("A4", "2^52-0.5", CalcValue::number(4_503_599_627_370_495.5)),
        (
            "A5",
            "-(2^52-0.5)",
            CalcValue::number(-4_503_599_627_370_495.5),
        ),
        ("A6", "1E15+0.5", CalcValue::number(1_000_000_000_000_000.5)),
        ("A7", "the largest binary64", CalcValue::number(f64::MAX)),
        (
            "A8",
            "minus the largest binary64",
            CalcValue::number(f64::MIN),
        ),
        ("A9", "2^63", CalcValue::number(9_223_372_036_854_775_808.0)),
        (
            "A10",
            "2^60",
            CalcValue::number(1_152_921_504_606_846_976.0),
        ),
        (
            "A11",
            "2^64",
            CalcValue::number(18_446_744_073_709_551_616.0),
        ),
        ("A12", "2^53-2", CalcValue::number(9_007_199_254_740_990.0)),
    ])
}

/// The rows of cfunc_m3_2__large_integers, run through calc_func::invoke on `s`, in the order the
/// rounding oracle workbook's sheet `large_integers` holds their formulas.
fn large_integers_rows(s: &mut TestSheet) -> Rows {
    const CASE: &str = "cfunc_m3_2__large_integers";
    const DOWN: &str = "INT rounds down to the nearest integer; an integer is unchanged";
    const TRUNCATED: &str = "TRUNC removes the fractional part; an integer is unchanged";
    const OVERFLOW: &str =
        "the exact result lies beyond binary64's range: #NUM!, the number rule's overflow";
    const AWAY: &str =
        "rounded up, away from zero, to an integer of that parity; one of that parity is unchanged";
    const INTEGER_PORTION: &str = "QUOTIENT is the integer portion of the division";
    let mut rows = Rows::new(CASE, "large_integers", false);
    let r = &mut rows;
    use WorksheetErrorCode::Num;
    let max = f64::MAX;

    // INT and TRUNC: a cast through a 64-bit integer would saturate from 2^63 on.
    rnd!(
        r,
        s,
        "INT",
        [cell(s, "A4")],
        N(4_503_599_627_370_495.0),
        Choice(
            "A4 is read as its binary64 4503599627370495.5 and rounded down; the 15-digit reading 4503599627370500 gives 4503599627370500"
        )
    );
    rnd!(
        r,
        s,
        "INT",
        [cell(s, "A5")],
        N(-4_503_599_627_370_496.0),
        Choice(
            "A5 is read as its binary64 -4503599627370495.5 and rounded down; the 15-digit reading -4503599627370500 gives -4503599627370500"
        )
    );
    rnd!(
        r,
        s,
        "INT",
        [cell(s, "A1")],
        N(9_007_199_254_740_992.0),
        Choice(
            "A1 is read as its binary64 2^53, an integer; the 15-digit reading 9007199254740990 gives 9007199254740990"
        )
    );
    rnd!(r, s, "INT", [cell(s, "A6")], N(1e15), Documented(DOWN));
    rnd!(
        r,
        s,
        "INT",
        [cell(s, "A9")],
        N(9_223_372_036_854_775_808.0),
        Choice(
            "A9 is read as its binary64 2^63, an integer; the 15-digit reading 9223372036854780000 gives its nearest binary64 9223372036854779904"
        )
    );
    rnd!(
        r,
        s,
        "INT",
        [cell(s, "A11")],
        N(18_446_744_073_709_551_616.0),
        Choice(
            "A11 is read as its binary64 2^64, an integer; the 15-digit reading 18446744073709600000 gives its nearest binary64 18446744073709600768"
        )
    );
    rnd!(r, s, "INT", [num("1E20")], N(1e20), Documented(DOWN));
    rnd!(
        r,
        s,
        "INT",
        [cell(s, "A7")],
        N(max),
        Choice(
            "A7 is read as the largest binary64, an integer; the 15-digit reading 1.79769313486232E308 lies beyond binary64's range and gives #NUM!"
        )
    );
    rnd!(
        r,
        s,
        "INT",
        [cell(s, "A8")],
        N(-max),
        Choice(
            "A8 is read as minus the largest binary64, an integer; the 15-digit reading -1.79769313486232E308 lies beyond binary64's range and gives #NUM!"
        )
    );
    rnd!(
        r,
        s,
        "TRUNC",
        [cell(s, "A5")],
        N(-4_503_599_627_370_500.0),
        Measured(
            "dc-bd5: TRUNC reads -4503599627370495.5 to 15 significant digits, -4503599627370500 (Excel 16.0.20430, cfunc_m3_2's capture)"
        )
    );
    rnd!(
        r,
        s,
        "TRUNC",
        [cell(s, "A6")],
        N(1e15),
        Documented(TRUNCATED)
    );
    rnd!(
        r,
        s,
        "TRUNC",
        [cell(s, "A9")],
        N(9_223_372_036_854_779_904.0),
        Measured(
            "dc-bd5: the 15-digit reading 9223372036854780000, whose nearest binary64 is 9223372036854779904 (Excel 16.0.20430, cfunc_m3_2's capture)"
        )
    );
    rnd!(
        r,
        s,
        "TRUNC",
        [cell(s, "A8")],
        N(f64::from_bits(0xfff0_0000_0000_000a)),
        Measured(
            "dc-j9k.18: the 15-digit reading -1.79769313486232E308 lies beyond binary64's range; Excel keeps its significand and carries the exponent into the field's top value, the NaN bits 0xfff000000000000a, which the cell holds as a number until the saved workbook (#NUM! in <v>) is reopened (Excel 16.0.20430, cfunc_m3_2's capture)"
        )
    );
    rnd!(
        r,
        s,
        "TRUNC",
        [num("9.99999999999999E307"), num("-307")],
        N(9e307),
        Choice(
            "a negative num_digits truncates to the left of the decimal point, as the ROUNDDOWN page states for ROUNDDOWN; the TRUNC page is silent on a negative num_digits. To -307 the digit of 10^307 is kept"
        )
    );
    rnd!(
        r,
        s,
        "TRUNC",
        [cell(s, "A7"), num("-308")],
        N(1e308),
        Choice(
            "a negative num_digits truncates to the left of the decimal point, as the ROUNDDOWN page states for ROUNDDOWN; the TRUNC page is silent on a negative num_digits. To -308 the digit of 10^308 of 1.797...E308 is kept, under either reading of A7"
        )
    );

    // ROUND, ROUNDUP and ROUNDDOWN at the boundaries.
    rnd!(
        r,
        s,
        "ROUND",
        [cell(s, "A4"), num("0")],
        N(4_503_599_627_370_500.0),
        Measured(
            "dc-bd5: the 15-digit reading of 4503599627370495.5 is 4503599627370500 (Excel 16.0.20430, cfunc_m3_2's capture)"
        )
    );
    rnd!(
        r,
        s,
        "ROUND",
        [cell(s, "A5"), num("0")],
        N(-4_503_599_627_370_500.0),
        Measured(
            "dc-bd5: the 15-digit reading of -4503599627370495.5 is -4503599627370500 (Excel 16.0.20430, cfunc_m3_2's capture)"
        )
    );
    rnd!(
        r,
        s,
        "ROUND",
        [cell(s, "A6"), num("0")],
        N(1e15),
        Measured(
            "dc-bd5: the 15-digit reading of 1E15+0.5 is 1E15 (Excel 16.0.20430, cfunc_m3_2's capture)"
        )
    );
    rnd!(
        r,
        s,
        "ROUND",
        [cell(s, "A1"), num("0")],
        N(9_007_199_254_740_990.0),
        Measured(
            "dc-bd5: the 15-digit reading of 2^53 is 9007199254740990 (Excel 16.0.20430, cfunc_m3_2's capture)"
        )
    );
    rnd!(
        r,
        s,
        "ROUND",
        [cell(s, "A3"), num("-1")],
        N(9_007_199_254_740_990.0),
        Documented(LEFT_OF_POINT)
    );
    rnd!(
        r,
        s,
        "ROUND",
        [num("999999999999999"), num("-1")],
        N(1e15),
        Documented(LEFT_OF_POINT)
    );
    rnd!(
        r,
        s,
        "ROUND",
        [cell(s, "A9"), num("-3")],
        N(9_223_372_036_854_779_904.0),
        Measured(
            "dc-bd5: the 15-digit reading 9223372036854780000, whose nearest binary64 is 9223372036854779904 (Excel 16.0.20430, cfunc_m3_2's capture)"
        )
    );
    rnd!(
        r,
        s,
        "ROUND",
        [cell(s, "A7"), num("0")],
        N(f64::from_bits(0x7ff0_0000_0000_000a)),
        Measured(
            "dc-j9k.18: the 15-digit reading 1.79769313486232E308 lies beyond binary64's range; Excel keeps its significand and carries the exponent into the field's top value, the NaN bits 0x7ff000000000000a, which the cell holds as a number until the saved workbook (#NUM! in <v>) is reopened (Excel 16.0.20430, cfunc_m3_2's capture)"
        )
    );
    rnd!(
        r,
        s,
        "ROUND",
        [num("9.99999999999999E307"), num("-307")],
        N(1e308),
        Documented(LEFT_OF_POINT)
    );
    rnd!(
        r,
        s,
        "ROUND",
        [cell(s, "A7"), num("-308")],
        N(f64::from_bits(0x7ff1_ccf3_85eb_c8a0)),
        Measured(
            "dc-j9k.18: 2E+308 past binary64's range, its significand kept and its exponent carried into the field's top value: the NaN bits 0x7ff1ccf385ebc8a0 (Excel 16.0.20430, cfunc_m3_2's capture)"
        )
    );
    rnd!(
        r,
        s,
        "ROUNDDOWN",
        [cell(s, "A7"), num("-308")],
        N(1e308),
        Documented("ROUNDDOWN rounds toward zero, here to 1E308")
    );
    rnd!(
        r,
        s,
        "ROUND",
        [num("1.5"), num("1E300")],
        N(0.0),
        Measured(
            "dc-bd5: num_digits is read as a 32-bit integer, and one of 2^31 or more reads as -2^31 (ROUND(1.5,1E+300) is 0 in Excel 16.0.20430, cfunc_m3_2's capture)"
        )
    );
    rnd!(
        r,
        s,
        "ROUND",
        [num("1.5"), num("-1E300")],
        N(0.0),
        Choice("a num_digits of -1E300: the nearest multiple of that power of ten is 0")
    );
    rnd!(
        r,
        s,
        "ROUNDUP",
        [num("1.5"), num("-1E300")],
        E(Num),
        Choice(OVERFLOW)
    );

    // MOD: a quotient of magnitude 1.1259E+12 or more is #NUM!, as Excel 16.0.20430 gives it
    // (dc-j9k measured the limit by bisection; the catalogue oracle's sheet dcj9k_math holds
    // MOD(1125899999999,1) = 0 and MOD(1125900000000,1) = #NUM!).
    rnd!(
        r,
        s,
        "MOD",
        [cell(s, "A1"), num("3")],
        E(Num),
        Measured(MOD_LIMIT)
    );
    rnd!(
        r,
        s,
        "MOD",
        [cell(s, "A3"), num("3")],
        E(Num),
        Measured(MOD_LIMIT)
    );
    rnd!(
        r,
        s,
        "MOD",
        [num("999999999999999"), num("2")],
        E(Num),
        Measured(MOD_LIMIT)
    );
    rnd!(
        r,
        s,
        "MOD",
        [num("1E15"), num("7")],
        E(Num),
        Measured(MOD_LIMIT)
    );
    rnd!(
        r,
        s,
        "MOD",
        [num("-1E15"), num("7")],
        E(Num),
        Measured(MOD_LIMIT)
    );
    rnd!(
        r,
        s,
        "MOD",
        [cell(s, "A10"), num("1024")],
        E(Num),
        Measured(MOD_LIMIT)
    );
    rnd!(
        r,
        s,
        "MOD",
        [cell(s, "A10"), num("3")],
        E(Num),
        Measured(MOD_LIMIT)
    );
    rnd!(
        r,
        s,
        "MOD",
        [num("1E300"), num("7")],
        E(Num),
        Measured(MOD_LIMIT)
    );
    rnd!(
        r,
        s,
        "MOD",
        [num("1E23"), num("1E8")],
        E(Num),
        Measured(MOD_LIMIT)
    );
    rnd!(
        r,
        s,
        "MOD",
        [cell(s, "A7"), num("3")],
        E(Num),
        Measured(MOD_LIMIT)
    );
    rnd!(
        r,
        s,
        "MOD",
        [cell(s, "A7"), cell(s, "A8")],
        N(0.0),
        Choice(NO_NEGATIVE_ZERO)
    );
    rnd!(
        r,
        s,
        "MOD",
        [num("-5"), num("9.99999999999999E307")],
        N(0.0),
        Measured(
            "dc-bd5: a remainder the divisor absorbs (-5 plus 9.99999999999999E+307 is the divisor) is 0 (Excel 16.0.20430, cfunc_m3_2's capture)"
        )
    );

    // QUOTIENT.
    rnd!(
        r,
        s,
        "QUOTIENT",
        [cell(s, "A1"), num("1")],
        N(9_007_199_254_740_992.0),
        Choice(
            "A1 is read as its binary64 2^53, an integer; the 15-digit reading gives 9007199254740990"
        )
    );
    rnd!(
        r,
        s,
        "QUOTIENT",
        [cell(s, "A3"), num("2")],
        N(4_503_599_627_370_497.0),
        Choice(
            "A3 is read as its binary64 2^53+2, whose half 4503599627370497 is exact; the 15-digit reading 9007199254740990 gives 4503599627370495"
        )
    );
    rnd!(
        r,
        s,
        "QUOTIENT",
        [num("1E16"), num("3")],
        N(3_333_333_333_333_333.0),
        Documented(INTEGER_PORTION)
    );
    rnd!(
        r,
        s,
        "QUOTIENT",
        [num("-1E16"), num("3")],
        N(-3_333_333_333_333_333.0),
        Documented(INTEGER_PORTION)
    );
    rnd!(
        r,
        s,
        "QUOTIENT",
        [num("1E20"), num("3")],
        N(33_333_333_333_333_333_333.0),
        Choice("the integer part 33333333333333333333 rounds once to a binary64")
    );
    rnd!(
        r,
        s,
        "QUOTIENT",
        [cell(s, "A7"), num("0.5")],
        E(Num),
        Choice(OVERFLOW)
    );
    rnd!(
        r,
        s,
        "QUOTIENT",
        [num("1E300"), num("1E-10")],
        E(Num),
        Choice(OVERFLOW)
    );

    // EVEN and ODD: no parity test through a 64-bit integer.
    rnd!(
        r,
        s,
        "EVEN",
        [cell(s, "A2")],
        N(9_007_199_254_740_992.0),
        Choice(
            "A2 is read as its binary64 2^53-1, odd, and rounded up to 2^53; the 15-digit reading 9007199254740990 is even and unchanged"
        )
    );
    rnd!(
        r,
        s,
        "EVEN",
        [cell(s, "A5")],
        N(-4_503_599_627_370_496.0),
        Choice(
            "A5 is read as its binary64 -4503599627370495.5, rounded away from zero to the even -4503599627370496; the 15-digit reading -4503599627370500 is even and unchanged"
        )
    );
    rnd!(
        r,
        s,
        "EVEN",
        [cell(s, "A1")],
        N(9_007_199_254_740_992.0),
        Choice(
            "A1 is read as its binary64 2^53, even and unchanged; the 15-digit reading gives 9007199254740990"
        )
    );
    rnd!(
        r,
        s,
        "EVEN",
        [num("999999999999999")],
        N(1e15),
        Documented(AWAY)
    );
    rnd!(
        r,
        s,
        "EVEN",
        [cell(s, "A9")],
        N(9_223_372_036_854_775_808.0),
        Choice(
            "A9 is read as its binary64 2^63, even and unchanged; the 15-digit reading gives 9223372036854779904"
        )
    );
    rnd!(
        r,
        s,
        "EVEN",
        [cell(s, "A7")],
        N(max),
        Choice(
            "A7 is read as the largest binary64, an even integer, unchanged; the 15-digit reading lies beyond binary64's range and gives #NUM!"
        )
    );
    rnd!(
        r,
        s,
        "EVEN",
        [cell(s, "A8")],
        N(-max),
        Choice(
            "A8 is read as minus the largest binary64, an even integer, unchanged; the 15-digit reading lies beyond binary64's range and gives #NUM!"
        )
    );
    rnd!(
        r,
        s,
        "ODD",
        [cell(s, "A2")],
        N(9_007_199_254_740_991.0),
        Documented(AWAY)
    );
    rnd!(
        r,
        s,
        "ODD",
        [cell(s, "A12")],
        N(9_007_199_254_740_991.0),
        Documented(AWAY)
    );
    rnd!(
        r,
        s,
        "ODD",
        [cell(s, "A4")],
        N(4_503_599_627_370_497.0),
        Choice(
            "A4 is read as its binary64 4503599627370495.5, rounded up to 4503599627370496 and on to the odd 4503599627370497; the 15-digit reading 4503599627370500 gives 4503599627370501"
        )
    );
    rnd!(
        r,
        s,
        "ODD",
        [num("1E15")],
        N(1_000_000_000_000_001.0),
        Documented(AWAY)
    );
    rnd!(
        r,
        s,
        "ODD",
        [cell(s, "A1")],
        N(9_007_199_254_740_992.0),
        Choice(
            "the odd integer 2^53+1 is no binary64; the addition rounds it, ties to even, to 2^53, an even result, departing from the page's nearest odd integer; the 15-digit reading 9007199254740990 gives 9007199254740991"
        )
    );
    rnd!(
        r,
        s,
        "ODD",
        [cell(s, "A3")],
        N(9_007_199_254_740_996.0),
        Choice(
            "the odd integer 2^53+3 rounds, ties to even, to 2^53+4, an even result, departing from the page's nearest odd integer; the 15-digit reading 9007199254740990 gives 9007199254740991"
        )
    );
    rnd!(
        r,
        s,
        "ODD",
        [cell(s, "A9")],
        N(9_223_372_036_854_775_808.0),
        Choice(
            "the odd integer 2^63+1 rounds to 2^63, an even result, departing from the page's nearest odd integer; the 15-digit reading gives 9223372036854779904"
        )
    );
    rnd!(
        r,
        s,
        "ODD",
        [cell(s, "A7")],
        N(max),
        Choice(
            "the odd integer above the largest binary64 rounds back to it, an even result, departing from the page's nearest odd integer; the 15-digit reading lies beyond binary64's range and gives #NUM!"
        )
    );

    // CEILING, FLOOR and MROUND.
    rnd!(
        r,
        s,
        "CEILING",
        [cell(s, "A4"), num("1")],
        N(4_503_599_627_370_500.0),
        Measured(
            "dc-j9k: the quotient 4503599627370495.5 is counted at 15 significant digits, 4503599627370500, as Excel 16.0.20430 gives CEILING(2^52-0.5,1)"
        )
    );
    rnd!(
        r,
        s,
        "FLOOR",
        [cell(s, "A5"), num("1")],
        N(-4503599627370500.0),
        Measured(
            "dc-j9k: the quotient is counted at 15 significant digits, as Excel 16.0.20430 counts it (the catalogue oracle's sheet dcj9k_math)"
        )
    );
    rnd!(
        r,
        s,
        "FLOOR",
        [cell(s, "A7"), num("1")],
        E(Num),
        Measured(
            "dc-j9k: the quotient is counted at 15 significant digits, which lies beyond binary64's range: #NUM!, as Excel 16.0.20430 gives FLOOR of the largest binary64 by 1"
        )
    );
    rnd!(
        r,
        s,
        "FLOOR",
        [cell(s, "A10"), num("1024")],
        N(1.1529215046068429e18),
        Measured(
            "dc-j9k: the quotient is counted at 15 significant digits, as Excel 16.0.20430 counts it (the catalogue oracle's sheet dcj9k_math)"
        )
    );
    rnd!(
        r,
        s,
        "CEILING",
        [cell(s, "A7"), num("9.99999999999999E307")],
        E(Num),
        Choice(OVERFLOW)
    );
    rnd!(
        r,
        s,
        "FLOOR",
        [cell(s, "A8"), num("9.99999999999999E307")],
        E(Num),
        Choice(OVERFLOW)
    );
    rnd!(
        r,
        s,
        "CEILING",
        [num("1E-300"), num("1E300")],
        N(0.0),
        Choice(
            "the binary64 order: the quotient 1E-600 lies below binary64's range and rounds to 0, whose integer above is 0, so the result is 0; the page's next multiple of 1E300 above 1E-300 is 1E300"
        )
    );
    rnd!(
        r,
        s,
        "FLOOR",
        [num("-1E-300"), num("1E300")],
        N(0.0),
        Choice(
            "the binary64 order: the quotient -1E-600 lies below binary64's range and rounds to -0, whose integer below is -0, so the result is a zero, published as +0; the page's multiple of 1E300 below -1E-300, away from zero, is -1E300"
        )
    );
    rnd!(
        r,
        s,
        "MROUND",
        [cell(s, "A6"), num("1")],
        N(1_000_000_000_000_001.0),
        Choice(
            "A6 is read as its binary64 1000000000000000.5, which is also the binary64 quotient: a half, rounded away from zero; the 15-digit reading 1E15 gives 1E15"
        )
    );
    rnd!(
        r,
        s,
        "MROUND",
        [cell(s, "A1"), num("3")],
        N(9_007_199_254_740_992.0),
        Choice(
            "the binary64 quotient of 2^53 by 3 is 3002399751580330.5, rounded away from zero to 3002399751580331 multiples, 9007199254740993, which rounds, ties to even, to 2^53; the 15-digit reading 9007199254740990 gives 9007199254740990"
        )
    );
    rnd!(
        r,
        s,
        "MROUND",
        [cell(s, "A7"), num("0.5")],
        E(Num),
        Choice(
            "the binary64 quotient of the largest binary64 by 0.5 overflows: #NUM!, as under the 15-digit reading of A7; the exact nearest multiple is A7 itself"
        )
    );

    // ABS and SIGN.
    rnd!(
        r,
        s,
        "ABS",
        [cell(s, "A8")],
        N(max),
        Choice(
            "A8 is read as minus the largest binary64; the 15-digit reading lies beyond binary64's range and gives #NUM!"
        )
    );
    rnd!(
        r,
        s,
        "ABS",
        [cell(s, "A5")],
        N(4_503_599_627_370_495.5),
        Choice(
            "A5 is read as its binary64 -4503599627370495.5; the 15-digit reading gives 4503599627370500"
        )
    );
    rnd!(
        r,
        s,
        "SIGN",
        [cell(s, "A8")],
        N(-1.0),
        Documented("SIGN returns -1 if the number is negative")
    );
    rows
}

#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test)]
fn cfunc_m3_2__large_integers() {
    const CASE: &str = "cfunc_m3_2__large_integers";
    let mut sheet = large_integers_sheet();
    note!("ACCEPT-NOTE: {CASE}: the sheet holds {}", sheet.described());
    let rows = large_integers_rows(&mut sheet);
    assert_workbook_sheet(ROUNDING_WORKBOOK, ROUNDING_WORKBOOK_PATH, &rows, &sheet);
    assert_rounding_workbook_generated(CASE);
    rounding_rows_note(&rows);
}

// ---------------------------------------------------------------------------------------------
// cfunc-m3.5: the conditional aggregates (COUNTIF, COUNTIFS, SUMIF, SUMIFS, AVERAGEIF, AVERAGEIFS,
// MAXIFS and MINIFS) and their criteria language through calc_func::invoke on catalogue targets.
//
// Each oracle row runs one call and asserts a value derived here by hand, and says how, as the
// cfunc-m3.4 rows do. A `documented` row applies a rule or an example that Microsoft's page for
// the function states, named in the row. An `unmeasured choice` row applies a rule that no page
// settles, or one this implementation follows against its page (TRUE in a SUMIFS or AVERAGEIFS
// target, a blank cell of an AVERAGEIFS criteria range, a logical cell of an AVERAGEIF range),
// each such disagreement listed as divergent in the per-ID manifest of
// `migration/ports/cfunc-m3.5.json`. A `refused` row ends in a fault, listed there as
// unsupported, and names the bead that owns the behaviour it refuses, never cfunc-m3.5 itself.
// Where a choice cites Excel, it cites Excel 16.0's ad-hoc COM read of a scratch copy of the oracle
// workbook, or of probe cells written into such a copy, on this machine (2026-10-21), which is not
// a capture and is not compared through calc_verdict.
//
// `tests/fixtures/cfunc_m3_5/oracle.xlsx`, written by the committed generator
// `tests/fixtures/make-fixtures.ps1`, holds every row's formula in column D of a sheet named after
// its case, in the order the case runs its rows, beside the cells the rows read; each case checks
// that its sheet holds exactly those formulas and cells. cfunc_m3_5__excel_comparison compares
// the rows of both sheets with Excel's capture of that workbook through calc_verdict::judge: each
// computed row must be matched, and each refused row not modelled with its owner. Until
// `cargo xtask capture` writes that capture to `tests/captures/cfunc_m3_5/oracle.capture.json`,
// the path holds the absent-capture marker and the case prints ACCEPT-NOT-RUN with calc-verdict's
// MissingCapture, so the runner records it as not run. The other two cases do not wait for the
// capture: their green shows the rules as written here, not agreement with Excel.

/// The conditional oracle workbook, as committed.
const CONDITIONAL_WORKBOOK: &[u8] = include_bytes!("fixtures/cfunc_m3_5/oracle.xlsx");
const CONDITIONAL_WORKBOOK_PATH: &str = "crates/calc-func/tests/fixtures/cfunc_m3_5/oracle.xlsx";
/// Excel's capture of the conditional oracle workbook, or the absent-capture marker until it is
/// taken.
const CONDITIONAL_CAPTURE: &[u8] = include_bytes!("captures/cfunc_m3_5/oracle.capture.json");
/// The case the conditional capture is recorded under.
const CONDITIONAL_CAPTURE_CASE: &str = "cfunc_m3_5__excel_comparison";
/// The command that writes the conditional capture.
const CONDITIONAL_CAPTURE_COMMAND: &str = "cargo xtask capture --fixture crates/calc-func/tests/fixtures/cfunc_m3_5/oracle.xlsx --case cfunc_m3_5__excel_comparison --out crates/calc-func/tests/captures/cfunc_m3_5/oracle.capture.json";
/// The bead's migration record, whose per-ID manifest the criteria case checks against the
/// catalogue.
const CONDITIONAL_RECORD: &str = include_str!("../../../migration/ports/cfunc-m3.5.json");

/// The family's entries: each spelling and the member number of its FunctionId in family 7.
const CONDITIONALS: [(&str, u32); 8] = [
    ("COUNTIF", 1),
    ("COUNTIFS", 2),
    ("SUMIF", 3),
    ("SUMIFS", 4),
    ("AVERAGEIF", 5),
    ("AVERAGEIFS", 6),
    ("MAXIFS", 7),
    ("MINIFS", 8),
];

/// Runs a row of cfunc-m3.5: `cnd!(rows, sheet, "SPELLING", [args], expect, basis)`.
macro_rules! cnd {
    ($rows:expr, $sheet:expr, $spelling:expr, [$($arg:expr),* $(,)?], $expect:expr, $basis:expr) => {{
        let args = [$($arg),*];
        basis_row("cfunc-m3.5", $rows, $sheet, named($spelling), &args, $expect, $basis)
    }};
}

/// Records `owner` as the bead that owns the behaviour the row just run refuses, so the Excel
/// comparison publishes that row as not modelled with this owner: counted, never matched.
fn owned(rows: &mut Rows, owner: Owner) {
    let row = rows.oracle.last_mut().expect("a row just run");
    assert!(
        row.outcome.is_err(),
        "{}: only a refused row names an owner",
        row.formula
    );
    row.owner = Some(owner);
}

/// The behaviours that refused rows of cfunc-m3.5 show, by the bead that owns each. No row names
/// cfunc-m3.5 itself: the Excel comparison counts such a row as a problem.

/// Prints how many rows a case ran and on what basis each expects its value.
fn conditional_rows_note(rows: &Rows) {
    let (choices, refused) = (rows.choices(), rows.refused());
    let mut owners: BTreeMap<&str, usize> = BTreeMap::new();
    for row in &rows.oracle {
        assert_eq!(
            row.owner.is_some(),
            row.outcome.is_err(),
            "{}: a refused row names its owner, a computed row none",
            row.formula
        );
        if let Some(owner) = row.owner {
            *owners.entry(owner.bead).or_default() += 1;
        }
    }
    let owners: Vec<String> = owners.iter().map(|(b, c)| format!("{c} {b}")).collect();
    note!(
        "ACCEPT-NOTE: {}: {} rows through calc_func::invoke, each asserted against a value or a refusal derived here by hand and each with its formula on sheet {} of {CONDITIONAL_WORKBOOK_PATH}: {} from documented rules, {} from dc-e00t's rule measured in Excel, {} unmeasured choices, {} refused (owned by: {}); none is compared with Excel here: cfunc_m3_5__excel_comparison compares every row with Excel's committed capture of that workbook",
        rows.case,
        rows.oracle.len(),
        rows.sheet,
        rows.oracle.len() - choices - refused - rows.measured,
        rows.measured,
        choices,
        refused,
        owners.join(", ")
    );
}

/// Checks that the committed conditional oracle workbook is its generator's output, on the route
/// where the generator runs.
fn assert_conditional_workbook_generated(case: &str) {
    #[cfg(all(windows, not(target_arch = "wasm32")))]
    {
        let generated = generated_fixtures();
        let names: Vec<&str> = generated.keys().map(String::as_str).collect();
        assert_eq!(
            names, GENERATED_WORKBOOKS,
            "the generator writes the oracle workbooks and nothing else"
        );
        let regenerated = &generated["cfunc_m3_5/oracle.xlsx"];
        assert!(
            regenerated.as_slice() == CONDITIONAL_WORKBOOK,
            "the committed conditional oracle workbook is not the generator's output (sha256 {} committed, {} generated)",
            sha256_hex(CONDITIONAL_WORKBOOK),
            sha256_hex(regenerated)
        );
        note!(
            "ACCEPT-NOTE: {case}: crates/calc-func/tests/fixtures/make-fixtures.ps1, run now under Windows PowerShell, wrote the committed conditional oracle workbook byte for byte ({} bytes)",
            regenerated.len()
        );
    }
    #[cfg(not(all(windows, not(target_arch = "wasm32"))))]
    {
        note!(
            "ACCEPT-NOTE: {case}: the generator runs under Windows PowerShell, so the check that the committed conditional oracle workbook is its output runs on the windows route; here the workbook is checked against the rows only"
        );
    }
}

/// What `"op"&A1` gives: calc_func::invoke of the catalogue's & entry on the operator's text and
/// the value of the cell, which the caller dereferences for &'s value parameter, as the formula's
/// own evaluation joins them before the conditional call receives the result.
fn join(sheet: &TestSheet, op: &str, a1: &str) -> Result<CalcValue, FunctionFault> {
    run(
        surface(Notation::Infix, "&"),
        &mut Recorded::new(vec![text_value(op), sheet.value(a1)]),
        &NumericMode::Fast,
    )
}

/// A criterion written as text joined to a cell's value, as `"op"&A1` writes it, with the value
/// [`join`] gives.
fn joined(sheet: &TestSheet, op: &str, a1: &str) -> Arg {
    let value = join(sheet, op, a1)
        .unwrap_or_else(|fault| panic!("\"{op}\"&{a1}: the & entry refuses: {fault:?}"));
    Arg {
        reads: sheet.entries(&[a1]),
        ..arg(&format!("\"{op}\"&{a1}"), value)
    }
}

// Rules the rows apply, as the functions' pages state them.
const IGNORES_CASE: &str =
    "matching ignores case: \"apples\" and \"APPLES\" match the same cells (the COUNTIF page)";
const ONE_CHARACTER: &str = "? matches any single character";
const TILDE: &str = "a ~ before * or ? finds that character itself";
const EMPTY_IS_ZERO: &str = "an empty cell given as the criterion is treated as 0 (the COUNTIFS, AVERAGEIF, MAXIFS and MINIFS pages)";
const EMPTY_IS_ZERO_ELSEWHERE: &str = "an empty cell given as the criterion is treated as 0, as the COUNTIFS, AVERAGEIF, MAXIFS and MINIFS pages state for those functions; this function's page is silent";
const NUMBER_TEXT_MEETS: &str = "number text in the range meets = a number as the number it reads as, never meets <, <=, > or >= a number, and always meets <> a number; no page states it (Excel's ad-hoc read agrees on every such row)";
const NOT_EQUAL: &str = "<> holds wherever = does not: blank cells, numbers, logicals and errors meet a <> text criterion";
const TOP_LEFT: &str = "a third range of another size is used from its top-left cell over the range's size (the SUMIF and AVERAGEIF pages)";
const SHAPED_PAST_MEASURED: &str = "dc-b1l: the third argument is shaped from its top-left cell to the range's size (the SUMIF and AVERAGEIF pages), past the reference the formula gives; Excel 16.0.20430's capture of this row gives this value";
const SAME_SHAPE: &str =
    "every range has the same size and shape; positions align even when the ranges do not";
const SHAPE_VALUE: &str = "ranges of different sizes or shapes return #VALUE!, as the MAXIFS page states for MAXIFS; this page names no error";
const SKIPPED: &str =
    "text and logicals in the target are skipped, as SUM skips them in a reference";
const MATCHED_ERROR: &str =
    "an error in a matched target cell is the result, the first in row-major order";
const DIGIT_LABEL_CRITERION: &str = "a criterion holding a word that no number text holds, such as the label Q1 or 2016-17 Winter, is no number (dc-e00t's rule, from its measurement of the words Excel reads inside number text, compared with its Excel capture in dc_e00t__excel_comparison), so it asks for that text, which a text criterion matches ignoring case (the COUNTIF page); no cell of A1:A16 holds it";
const DIGIT_LABEL_CELL: &str = "the label 2016-17 Winter in the range holds a word that no number text holds, so it is no number (dc-e00t's rule, compared with its Excel capture in dc_e00t__excel_comparison) and does not meet = 2016";
const NUMBER_TEXT_FORMS: &str = "a criterion that Excel's number-text rules may read as a number, such as the fraction or date 1/2, is not modelled; owned by dc-uar";

// ---------------------------------------------------------------------------------------------
// cfunc_m3_5__criteria_semantics

/// The cells the criteria rows read. A1:A16 hold one value of each kind the criteria language
/// tells apart (A8 is blank, A9 holds empty text); B1:B16 hold the powers of two 1 to 32768, so a
/// sum names the rows it added; C1:C16 hold their row numbers. A18:A22 and B18:B22 hold a target
/// with number text, a logical and an error; A23 holds non-ASCII text, A24 a number within a
/// relative 1e-13 of 0.3, A25 a label holding digits, A26 text holding a ~, A27 FALSE and A28
/// #VALUE!.
fn criteria_semantics_sheet() -> TestSheet {
    let mut owned: Vec<(String, String, CalcValue)> = vec![
        ("A1".into(), "text \"apple\"".into(), text_value("apple")),
        (
            "A2".into(),
            "text \"Apple Pie\"".into(),
            text_value("Apple Pie"),
        ),
        ("A3".into(), "text \"banana\"".into(), text_value("banana")),
        ("A4".into(), "5".into(), CalcValue::number(5.0)),
        ("A5".into(), "text \"5\"".into(), text_value("5")),
        ("A6".into(), "10".into(), CalcValue::number(10.0)),
        ("A7".into(), "TRUE".into(), CalcValue::logical(true)),
        ("A9".into(), "empty text".into(), text_value("")),
        ("A10".into(), "text \"a*b\"".into(), text_value("a*b")),
        ("A11".into(), "text \"axb\"".into(), text_value("axb")),
        (
            "A12".into(),
            "#N/A".into(),
            CalcValue::error(WorksheetErrorCode::NA),
        ),
        ("A13".into(), "text \"abc\"".into(), text_value("abc")),
        ("A14".into(), "text \"a?c\"".into(), text_value("a?c")),
        ("A15".into(), "0".into(), CalcValue::number(0.0)),
        ("A16".into(), "text \"APPLE\"".into(), text_value("APPLE")),
    ];
    for row in 1..=16u32 {
        let power = 2f64.powi(row as i32 - 1);
        owned.push((
            format!("B{row}"),
            format!("{power}"),
            CalcValue::number(power),
        ));
        owned.push((
            format!("C{row}"),
            format!("{row}"),
            CalcValue::number(f64::from(row)),
        ));
    }
    owned.extend([
        ("A18".into(), "text \"k\"".into(), text_value("k")),
        ("A19".into(), "text \"k\"".into(), text_value("k")),
        ("A20".into(), "text \"k\"".into(), text_value("k")),
        ("A21".into(), "text \"k\"".into(), text_value("k")),
        ("A22".into(), "text \"j\"".into(), text_value("j")),
        (
            "A23".into(),
            "text \"caf\\u{e9}\"".into(),
            text_value("caf\u{e9}"),
        ),
        (
            "A24".into(),
            "0.30000000000000004".into(),
            CalcValue::number(0.300_000_000_000_000_04),
        ),
        (
            "A25".into(),
            "text \"2016-17 Winter\"".into(),
            text_value("2016-17 Winter"),
        ),
        ("A26".into(), "text \"a~b\"".into(), text_value("a~b")),
        ("A27".into(), "FALSE".into(), CalcValue::logical(false)),
        (
            "A28".into(),
            "#VALUE!".into(),
            CalcValue::error(WorksheetErrorCode::Value),
        ),
        ("B18".into(), "3".into(), CalcValue::number(3.0)),
        ("B19".into(), "text \"4\"".into(), text_value("4")),
        ("B20".into(), "TRUE".into(), CalcValue::logical(true)),
        (
            "B21".into(),
            "#DIV/0!".into(),
            CalcValue::error(WorksheetErrorCode::Div0),
        ),
        ("B22".into(), "100".into(), CalcValue::number(100.0)),
    ]);
    let cells: Vec<(&str, &str, CalcValue)> = owned
        .iter()
        .map(|(address, entry, value)| (address.as_str(), entry.as_str(), value.clone()))
        .collect();
    TestSheet::new(&cells)
}

/// The rows of cfunc_m3_5__criteria_semantics, run through calc_func::invoke on `s`, in the order
/// the conditional oracle workbook's sheet `criteria_semantics` holds their formulas.
fn criteria_semantics_rows(s: &mut TestSheet, quiet: bool) -> Rows {
    const CASE: &str = "cfunc_m3_5__criteria_semantics";
    let mut rows = Rows::new(CASE, "criteria_semantics", quiet);
    let r = &mut rows;
    use WorksheetErrorCode::{Div0, NA};
    let a = |s: &TestSheet| reference(s, &["A1:A16"]);
    let b = |s: &TestSheet| reference(s, &["B1:B16"]);
    let c = |s: &TestSheet| reference(s, &["C1:C16"]);

    // COUNTIF: text, patterns and escapes.
    cnd!(
        r,
        s,
        "COUNTIF",
        [a(s), txt("apple")],
        N(2.0),
        Documented(IGNORES_CASE)
    );
    cnd!(
        r,
        s,
        "COUNTIF",
        [a(s), txt("=apple")],
        N(2.0),
        Documented(
            "= before text asks for equal text, as the COUNTIFS page's criterion \"=Yes\" does"
        )
    );
    cnd!(
        r,
        s,
        "COUNTIF",
        [a(s), txt("<>apple")],
        N(14.0),
        Choice(NOT_EQUAL)
    );
    cnd!(
        r,
        s,
        "COUNTIF",
        [a(s), txt("app*")],
        N(3.0),
        Documented("* matches any sequence of characters: apple, Apple Pie and APPLE")
    );
    cnd!(
        r,
        s,
        "COUNTIF",
        [a(s), txt("*")],
        N(10.0),
        Choice(
            "* matches every text, A5's number text and A9's empty text included; numbers, logicals, errors and blank cells are not text (the page: \"*\" counts the cells containing any text)"
        )
    );
    cnd!(
        r,
        s,
        "COUNTIF",
        [a(s), txt("?????")],
        N(2.0),
        Documented("? matches any single character: the five-character apple and APPLE")
    );
    cnd!(
        r,
        s,
        "COUNTIF",
        [a(s), txt("a*b")],
        N(2.0),
        Documented("* matches any sequence of characters: a*b and axb")
    );
    cnd!(
        r,
        s,
        "COUNTIF",
        [a(s), txt("a~*b")],
        N(1.0),
        Documented(TILDE)
    );
    cnd!(
        r,
        s,
        "COUNTIF",
        [a(s), txt("a?c")],
        N(2.0),
        Documented("? matches any single character: abc and a?c")
    );
    cnd!(
        r,
        s,
        "COUNTIF",
        [a(s), txt("a~?c")],
        N(1.0),
        Documented(TILDE)
    );

    // COUNTIF: numbers, blanks, logicals, errors and cell criteria.
    cnd!(
        r,
        s,
        "COUNTIF",
        [a(s), num("5")],
        N(2.0),
        Choice(NUMBER_TEXT_MEETS)
    );
    cnd!(
        r,
        s,
        "COUNTIF",
        [a(s), txt("5")],
        N(2.0),
        Choice(NUMBER_TEXT_MEETS)
    );
    cnd!(
        r,
        s,
        "COUNTIF",
        [a(s), txt(">=5")],
        N(2.0),
        Choice(
            "5 and 10 meet >=5; text \"5\" does not, since an ordering never reads text as a number (Excel's ad-hoc read: 2)"
        )
    );
    cnd!(
        r,
        s,
        "COUNTIF",
        [a(s), txt("<5")],
        N(1.0),
        Choice("only the number 0 meets <5: blank cells and TRUE do not meet a number criterion")
    );
    cnd!(
        r,
        s,
        "COUNTIF",
        [a(s), txt("<>5")],
        N(15.0),
        Choice(
            "<> a number holds for every cell but that number: text \"5\" meets <>5 although it meets 5, and so do the other texts, TRUE, #N/A and the blank A8 (Excel's ad-hoc read: 15)"
        )
    );
    cnd!(
        r,
        s,
        "COUNTIF",
        [a(s), joined(s, "=", "A16")],
        N(2.0),
        Documented(
            "a criterion may join an operator to a cell's value with &, as the page's example COUNTIF(B2:B5,\"<>\"&B4) does; calc_func's & entry joins =APPLE here, and =4 over a number in row D91"
        )
    );
    cnd!(
        r,
        s,
        "COUNTIF",
        [a(s), txt("")],
        N(2.0),
        Choice(
            "\"\" matches a blank cell, as the SUMIF page's example SUMIF(A2:A7,\"\",C2:C7) shows, and empty text, which no page settles"
        )
    );
    cnd!(
        r,
        s,
        "COUNTIF",
        [a(s), txt("=")],
        N(1.0),
        Choice("= alone matches blank cells only, not empty text")
    );
    cnd!(
        r,
        s,
        "COUNTIF",
        [a(s), txt("<>")],
        N(15.0),
        Choice("<> alone matches every cell but a blank one, empty text included")
    );
    cnd!(
        r,
        s,
        "COUNTIF",
        [a(s), logical(true)],
        N(1.0),
        Choice("a logical criterion matches that logical only")
    );
    cnd!(
        r,
        s,
        "COUNTIF",
        [a(s), txt("TRUE")],
        N(1.0),
        Choice("the text TRUE asks for the logical TRUE")
    );
    cnd!(
        r,
        s,
        "COUNTIF",
        [a(s), txt("#N/A")],
        N(1.0),
        Choice("the text of an error literal asks for cells holding that error")
    );
    cnd!(
        r,
        s,
        "COUNTIF",
        [a(s), txt("<>#N/A")],
        N(15.0),
        Choice("<> an error literal holds for every cell but those holding that error")
    );
    cnd!(
        r,
        s,
        "COUNTIF",
        [a(s), reference(s, &["A8"])],
        N(1.0),
        Choice(EMPTY_IS_ZERO_ELSEWHERE)
    );
    cnd!(
        r,
        s,
        "COUNTIF",
        [a(s), reference(s, &["A1"])],
        N(2.0),
        Documented(
            "a cell given as the criterion asks for its value, as the page's example COUNTIF(A2:A5,A4) does"
        )
    );
    cnd!(
        r,
        s,
        "COUNTIF",
        [reference(s, &["A3:A6"]), txt("<b")],
        N(1.0),
        Choice(
            "text orders by the comparison operators' rule (digits before letters, a prefix first): text \"5\" is below b and banana is not; the numbers 5 and 10 do not meet a text criterion"
        )
    );
    cnd!(
        r,
        s,
        "COUNTIF",
        [a(s), txt("*5*")],
        N(1.0),
        Choice("a pattern matches text only: text \"5\" matches *5* and the number 5 does not")
    );
    cnd!(
        r,
        s,
        "COUNTIF",
        [a(s), txt("<>*")],
        N(6.0),
        Choice("<>* matches every cell that holds no text: 5, 10, TRUE, the blank A8, #N/A and 0")
    );

    // COUNTIF: an ordering through the comparison operators' collation, then refused criteria
    // and cells, and the label Q1, which is a text criterion.
    cnd!(
        r,
        s,
        "COUNTIF",
        [a(s), txt("<b")],
        N(9.0),
        Measured(
            "an ordering criterion orders text as the comparison operators do, by cdoc-jpha's collation (measured in Excel and compared with its capture in cdoc_jpha__excel_comparison): \"Apple Pie\", whose space weighs before the letters, is below b with eight other cells of A1:A16"
        )
    );
    cnd!(
        r,
        s,
        "COUNTIF",
        [a(s), txt("Q1")],
        N(0.0),
        Measured(DIGIT_LABEL_CRITERION)
    );
    cnd!(
        r,
        s,
        "COUNTIF",
        [a(s), txt("1/2")],
        Refused(Some(1)),
        Unsupported(NUMBER_TEXT_FORMS)
    );
    owned(r, NUMBER_TEXT);
    cnd!(
        r,
        s,
        "COUNTIF",
        [a(s), txt("#VALUE!")],
        N(0.0),
        Choice(
            "the literal #VALUE! asks for cells holding that error, as the other error literals do, and A1:A16 holds none (Excel's ad-hoc read: 0; 1 over a probe cell holding #VALUE!, row D97); the SUMIF page says only that matching the string #VALUE! gives incorrect results"
        )
    );
    cnd!(
        r,
        s,
        "COUNTIF",
        [a(s), txt("a~b")],
        N(0.0),
        Choice(
            "a ~ before a character other than *, ? or ~ is itself, so a~b asks for the text a~b, which A1:A16 does not hold (Excel's ad-hoc read: 0; over probe cells, 1 for the text a~b and 0 for ab, row D93)"
        )
    );
    cnd!(
        r,
        s,
        "COUNTIF",
        [a(s), err(NA)],
        N(1.0),
        Choice(
            "an error value given as the criterion asks for cells holding that error: A12 (Excel's ad-hoc read: 1)"
        )
    );
    cnd!(
        r,
        s,
        "COUNTIF",
        [a(s), omitted()],
        N(1.0),
        Choice(
            "an omitted criterion is 0, as an empty cell given as the criterion is: A15 (Excel's ad-hoc read: 1; row D92 tells 0 from a blank)"
        )
    );
    cnd!(
        r,
        s,
        "COUNTIF",
        [a(s), txt("<TRUE")],
        N(0.0),
        Choice(
            "an ordering against a logical compares logicals only, FALSE below TRUE: A7's TRUE is not below TRUE (Excel's ad-hoc read: 0; FALSE meets <TRUE, row D94)"
        )
    );
    cnd!(
        r,
        s,
        "COUNTIF",
        [a(s), txt(&"a".repeat(256))],
        E(WorksheetErrorCode::Value),
        Choice(
            "a criterion longer than 255 characters, operator included, is #VALUE! (Excel's ad-hoc read: #VALUE!, also for REPT(\"a\",256), for \"<>\" joined to 254 characters and over a single blank cell, while \"<>\" joined to 253 computes); the COUNTIF and SUMIF pages say only that such a criterion gives incorrect results"
        )
    );
    cnd!(
        r,
        s,
        "COUNTIF",
        [reference(s, &["A23"]), txt("caf\u{e9}")],
        N(1.0),
        Choice(
            "identical texts are equal, the comparison operators' rule, whatever their characters"
        )
    );
    cnd!(
        r,
        s,
        "COUNTIF",
        [reference(s, &["A23"]), txt("CAF\u{c9}")],
        N(1.0),
        Measured(
            "text equality ignores case beyond ASCII too, as the comparison operators' equality does by cdoc-jpha's collation (measured in Excel and compared with its capture in cdoc_jpha__excel_comparison): caf\u{e9} meets CAF\u{c9}"
        )
    );
    cnd!(
        r,
        s,
        "COUNTIF",
        [reference(s, &["A22:A23"]), txt("*")],
        N(2.0),
        Choice("* alone matches every text, whatever its characters")
    );
    cnd!(
        r,
        s,
        "COUNTIF",
        [reference(s, &["A22:A23"]), txt("caf*")],
        N(1.0),
        Measured(
            "a pattern matches text beyond ASCII character by character, case folded (dc-ip6); this sheet's capture gives 1"
        )
    );
    cnd!(
        r,
        s,
        "COUNTIF",
        [reference(s, &["A24"]), num("0.3")],
        N(1.0),
        Measured(
            "a number criterion meets a number with the same 15-digit rounding, as the comparison operators do by cdoc-jpha's rule (measured in Excel and compared with its capture in cdoc_jpha__excel_comparison): 0.30000000000000004 meets 0.3"
        )
    );
    cnd!(
        r,
        s,
        "COUNTIF",
        [reference(s, &["A18:A22"]), txt("K")],
        N(4.0),
        Documented(IGNORES_CASE)
    );
    cnd!(
        r,
        s,
        "COUNTIF",
        [reference(s, &["B18:B22"]), txt(">0")],
        N(2.0),
        Choice("3 and 100 meet >0; text \"4\", TRUE and #DIV/0! do not (Excel's ad-hoc read: 2)")
    );

    // SUMIF.
    cnd!(
        r,
        s,
        "SUMIF",
        [a(s), txt("apple"), b(s)],
        N(32769.0),
        Documented(
            "the sum_range cells of the rows whose range cell meets the criterion are added: rows 1 and 16"
        )
    );
    cnd!(
        r,
        s,
        "SUMIF",
        [a(s), txt("a*"), b(s)],
        N(46595.0),
        Documented("* matches any sequence of characters: rows 1, 2, 10, 11, 13, 14 and 16")
    );
    cnd!(
        r,
        s,
        "SUMIF",
        [a(s), txt(""), b(s)],
        N(384.0),
        Choice(
            "\"\" matches the blank A8, as the page's example SUMIF(A2:A7,\"\",C2:C7) shows, and A9's empty text"
        )
    );
    cnd!(
        r,
        s,
        "SUMIF",
        [a(s), txt("<>"), b(s)],
        N(65407.0),
        Choice("<> alone matches every row but the blank A8")
    );
    cnd!(
        r,
        s,
        "SUMIF",
        [a(s), num("5"), b(s)],
        N(24.0),
        Choice(NUMBER_TEXT_MEETS)
    );
    cnd!(
        r,
        s,
        "SUMIF",
        [a(s), txt("<>5"), b(s)],
        N(65527.0),
        Choice(
            "<>5 matches every row but row 4, which holds the number 5; text \"5\" and the blank A8 meet it: 65535 - 8 (Excel's ad-hoc read: 65527)"
        )
    );
    cnd!(
        r,
        s,
        "SUMIF",
        [a(s), txt(">=5")],
        N(15.0),
        Choice("5 and 10 meet >=5 and are added; text \"5\" does not meet it: 5 + 10")
    );
    cnd!(
        r,
        s,
        "SUMIF",
        [a(s), txt("<>#N/A"), b(s)],
        N(63487.0),
        Choice("<>#N/A matches every row but row 12")
    );
    cnd!(
        r,
        s,
        "SUMIF",
        [a(s), logical(true), b(s)],
        N(64.0),
        Choice("a logical criterion matches that logical only")
    );
    cnd!(
        r,
        s,
        "SUMIF",
        [a(s), reference(s, &["A8"]), b(s)],
        N(16384.0),
        Choice(EMPTY_IS_ZERO_ELSEWHERE)
    );
    cnd!(
        r,
        s,
        "SUMIF",
        [c(s), txt(">8")],
        N(100.0),
        Documented(
            "with sum_range omitted, the range's own cells that meet the criterion are added: 9 to 16"
        )
    );
    cnd!(
        r,
        s,
        "SUMIF",
        [c(s), txt(">8"), reference(s, &["B1"])],
        N(65280.0),
        Measured(SHAPED_PAST_MEASURED)
    );
    cnd!(
        r,
        s,
        "SUMIF",
        [c(s), txt("<3"), reference(s, &["B5:B6"])],
        N(48.0),
        Measured(SHAPED_PAST_MEASURED)
    );
    cnd!(
        r,
        s,
        "SUMIF",
        [
            reference(s, &["A18:A20"]),
            txt("k"),
            reference(s, &["B18:B20"])
        ],
        N(3.0),
        Choice(SKIPPED)
    );
    cnd!(
        r,
        s,
        "SUMIF",
        [
            reference(s, &["A18:A22"]),
            txt("k"),
            reference(s, &["B18:B22"])
        ],
        E(Div0),
        Choice(MATCHED_ERROR)
    );
    cnd!(
        r,
        s,
        "SUMIF",
        [
            reference(s, &["A18:A22"]),
            txt("j"),
            reference(s, &["B18:B22"])
        ],
        N(100.0),
        Choice("an error in a row the criterion rules out is not used")
    );

    // SUMIFS.
    cnd!(
        r,
        s,
        "SUMIFS",
        [b(s), a(s), txt("a*"), c(s), txt(">10")],
        N(46080.0),
        Documented("a cell is added when its row meets every criterion: rows 11, 13, 14 and 16")
    );
    cnd!(
        r,
        s,
        "SUMIFS",
        [b(s), a(s), txt("<>apple"), c(s), txt("<=4")],
        N(14.0),
        Choice(NOT_EQUAL)
    );
    cnd!(
        r,
        s,
        "SUMIFS",
        [b(s), a(s), txt("a?c")],
        N(12288.0),
        Documented(ONE_CHARACTER)
    );
    cnd!(
        r,
        s,
        "SUMIFS",
        [
            reference(s, &["B18:B20"]),
            reference(s, &["A18:A20"]),
            txt("k")
        ],
        N(3.0),
        Choice(
            "text and logicals in sum_range are skipped; the SUMIFS page says TRUE counts as 1 (divergent)"
        )
    );

    // COUNTIFS.
    cnd!(
        r,
        s,
        "COUNTIFS",
        [a(s), txt("a*"), c(s), txt(">10")],
        N(4.0),
        Documented("a position counts when its cells meet every criterion")
    );
    cnd!(
        r,
        s,
        "COUNTIFS",
        [a(s), txt("<>apple"), c(s), txt("<=4")],
        N(3.0),
        Choice(NOT_EQUAL)
    );
    cnd!(
        r,
        s,
        "COUNTIFS",
        [c(s), txt(">3"), c(s), txt("<6")],
        N(2.0),
        Documented(
            "one range may take two criteria, as the page's example COUNTIFS(A2:A7,\"<6\",A2:A7,\">1\") does"
        )
    );
    cnd!(
        r,
        s,
        "COUNTIFS",
        [a(s), txt("="), c(s), txt(">0")],
        N(1.0),
        Choice("= alone matches blank cells only")
    );

    // AVERAGEIF and AVERAGEIFS.
    cnd!(
        r,
        s,
        "AVERAGEIF",
        [c(s), txt(">12")],
        N(14.5),
        Documented(
            "with average_range omitted, the range's own cells that meet the criterion are averaged"
        )
    );
    cnd!(
        r,
        s,
        "AVERAGEIF",
        [a(s), txt("a*"), c(s)],
        N(67.0 / 7.0),
        Documented(
            "* matches any sequence of characters: the average of 1, 2, 10, 11, 13, 14 and 16, 67 divided by 7 once"
        )
    );
    cnd!(
        r,
        s,
        "AVERAGEIF",
        [a(s), txt("zzz"), b(s)],
        E(Div0),
        Documented("no cell meets the criterion: #DIV/0!")
    );
    cnd!(
        r,
        s,
        "AVERAGEIF",
        [a(s), txt(">=5")],
        N(7.5),
        Choice("5 and 10 meet >=5 and are averaged; text \"5\" does not meet it: (5 + 10) / 2")
    );
    cnd!(
        r,
        s,
        "AVERAGEIF",
        [
            reference(s, &["A18:A20"]),
            txt("k"),
            reference(s, &["B18:B20"])
        ],
        N(3.0),
        Choice(SKIPPED)
    );
    cnd!(
        r,
        s,
        "AVERAGEIFS",
        [c(s), a(s), txt("a*"), c(s), txt(">10")],
        N(13.5),
        Documented("a cell is averaged when its row meets every criterion: 11, 13, 14 and 16")
    );
    cnd!(
        r,
        s,
        "AVERAGEIFS",
        [b(s), a(s), txt("zzz")],
        E(Div0),
        Documented("no cell meets every criterion: #DIV/0!")
    );
    cnd!(
        r,
        s,
        "AVERAGEIFS",
        [
            reference(s, &["B18:B20"]),
            reference(s, &["A18:A20"]),
            txt("k")
        ],
        N(3.0),
        Choice(
            "text and logicals in average_range are skipped; the AVERAGEIFS page says TRUE counts as 1 (divergent)"
        )
    );
    cnd!(
        r,
        s,
        "AVERAGEIFS",
        [c(s), a(s), txt("<1")],
        N(15.0),
        Choice(
            "only A15's 0 meets <1, so the average is C15; the AVERAGEIFS page says an empty criteria-range cell is treated as 0, which would add the blank A8's row and give 11.5 (divergent)"
        )
    );

    // MAXIFS and MINIFS.
    cnd!(
        r,
        s,
        "MAXIFS",
        [b(s), a(s), txt("a*")],
        N(32768.0),
        Documented("the largest cell of max_range whose row meets the criterion")
    );
    cnd!(
        r,
        s,
        "MINIFS",
        [b(s), a(s), txt("a*")],
        N(1.0),
        Documented("the smallest cell of min_range whose row meets the criterion")
    );
    cnd!(
        r,
        s,
        "MAXIFS",
        [b(s), a(s), txt("zzz")],
        N(0.0),
        Documented("no cell meets the criteria: 0")
    );
    cnd!(
        r,
        s,
        "MINIFS",
        [c(s), a(s), txt(">=5")],
        N(4.0),
        Choice("rows 4 and 6 meet >=5; text \"5\" in row 5 does not")
    );
    cnd!(
        r,
        s,
        "MAXIFS",
        [c(s), a(s), reference(s, &["A8"])],
        N(15.0),
        Documented(EMPTY_IS_ZERO)
    );
    cnd!(
        r,
        s,
        "MAXIFS",
        [
            reference(s, &["B18:B22"]),
            reference(s, &["A18:A22"]),
            txt("k")
        ],
        E(Div0),
        Choice(MATCHED_ERROR)
    );
    cnd!(
        r,
        s,
        "MINIFS",
        [
            reference(s, &["B18:B22"]),
            reference(s, &["A18:A22"]),
            txt("j")
        ],
        N(100.0),
        Choice("an error in a row the criterion rules out is not used")
    );

    // A logical cell of an AVERAGEIF range meets a criterion as it does in the other functions.
    cnd!(
        r,
        s,
        "AVERAGEIF",
        [a(s), logical(true), b(s)],
        N(64.0),
        Choice(
            "TRUE in A7 meets the criterion TRUE, as in COUNTIF and SUMIF; the AVERAGEIF page says cells in range that contain TRUE or FALSE are ignored, which would give #DIV/0! (divergent; Excel's ad-hoc read: 64)"
        )
    );
    cnd!(
        r,
        s,
        "AVERAGEIF",
        [a(s), txt("<>apple"), b(s)],
        N(32766.0 / 14.0),
        Choice(
            "every row but 1 and 16 meets <>apple, TRUE in A7 included: 32766 divided by 14 once; the AVERAGEIF page's rule that TRUE and FALSE in range are ignored would leave row 7 out (divergent; Excel's ad-hoc read: 2340.4285714285716)"
        )
    );

    // A25 holds a label with digits, as the criteria built "="&cell in the Ofgem workbooks name.
    cnd!(
        r,
        s,
        "SUMIFS",
        [b(s), a(s), joined(s, "=", "A25")],
        N(0.0),
        Measured(DIGIT_LABEL_CRITERION)
    );
    cnd!(
        r,
        s,
        "COUNTIF",
        [reference(s, &["A23:A25"]), txt(">0")],
        N(1.0),
        Choice(
            "an ordering against a number never meets text, whatever it holds: only A24 meets >0, not the label 2016-17 Winter (Excel's ad-hoc read: 1)"
        )
    );
    cnd!(
        r,
        s,
        "COUNTIF",
        [reference(s, &["A25"]), txt("<>2016")],
        N(1.0),
        Choice(
            "<> a number holds for every text, the label 2016-17 Winter included (Excel's ad-hoc read: 1)"
        )
    );
    cnd!(
        r,
        s,
        "COUNTIF",
        [reference(s, &["A25"]), num("2016")],
        N(0.0),
        Measured(DIGIT_LABEL_CELL)
    );

    // A criterion joined over a number: & reads the number 4 as the text 4 (dc-q9rb).
    cnd!(
        r,
        s,
        "COUNTIF",
        [c(s), joined(s, "=", "C4")],
        N(1.0),
        Documented(
            "a criterion may join an operator to a cell's number with &, as the page's example COUNTIF(B2:B5,\"<>\"&B4) does over the number 75; & gives the number 4 as the text 4 (dc-q9rb's measurement, compared with its Excel capture), so the criterion is =4 and only C4 meets it"
        )
    );

    // Rows that tell the choices of the rows above apart where those rows' values do not.
    cnd!(
        r,
        s,
        "SUMIF",
        [a(s), omitted(), b(s)],
        N(16384.0),
        Choice(
            "an omitted criterion is 0: row 15's 0 meets it, not the blank A8 (128) or the empty text A9 (256) (Excel's ad-hoc read: 16384)"
        )
    );
    cnd!(
        r,
        s,
        "COUNTIF",
        [reference(s, &["A26"]), txt("a~b")],
        N(1.0),
        Choice(
            "a ~ before b is itself: a~b meets the text a~b (Excel's ad-hoc read over a probe cell holding a~b: 1)"
        )
    );
    cnd!(
        r,
        s,
        "COUNTIF",
        [reference(s, &["A27"]), txt("<TRUE")],
        N(1.0),
        Choice(
            "an ordering against a logical compares logicals, FALSE below TRUE (Excel's ad-hoc read over a probe cell holding FALSE: 1)"
        )
    );
    cnd!(
        r,
        s,
        "COUNTIF",
        [a(s), txt(">=")],
        N(0.0),
        Choice(
            "<, <=, > or >= with no operand meets no cell, not even the empty text A9 or the blank A8 (Excel's ad-hoc read: 0, and 0 for each of the four over A1:A16)"
        )
    );
    cnd!(
        r,
        s,
        "COUNTIF",
        [reference(s, &["B18:B22"]), txt("<#N/A")],
        N(1.0),
        Choice(
            "an ordering against an error literal compares errors only, #DIV/0! below #N/A; errors order as ERROR.TYPE numbers them, #NULL! 1 to #N/A 7, which is measured for this pair only (Excel's ad-hoc read: 1)"
        )
    );
    cnd!(
        r,
        s,
        "COUNTIF",
        [reference(s, &["A28"]), txt("#VALUE!")],
        N(1.0),
        Choice(
            "the literal #VALUE! asks for cells holding #VALUE! (Excel's ad-hoc read over a probe cell holding #VALUE!: 1)"
        )
    );
    rows
}

/// The pairs `(family 7 member, spelling)` the catalogue holds.
fn conditional_family() -> BTreeSet<(u32, String)> {
    catalogue()
        .targets()
        .filter(|target| target.id().raw() >> 16 == 7)
        .map(|target| (target.id().raw(), target.spelling().to_owned()))
        .collect()
}

/// One call of `spelling` on `values` through calc_func::invoke with `cells` as the reader.
fn conditional_call(
    spelling: &str,
    values: Vec<CalcValue>,
    mode: &NumericMode,
    cells: &mut dyn CellReader,
) -> Result<CalcValue, FunctionFault> {
    invoke(
        surface(Notation::Call, spelling),
        &mut Recorded::new(values),
        &mut EvalServices::with_cells(mode, cells),
        &mut FunctionCallScratch::new(),
    )
}

#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test)]
fn cfunc_m3_5__criteria_semantics() {
    const CASE: &str = "cfunc_m3_5__criteria_semantics";
    let fast = NumericMode::Fast;
    let mut sheet = criteria_semantics_sheet();
    note!(
        "ACCEPT-NOTE: {CASE}: the sheet holds {}; A8 is blank",
        sheet.described()
    );
    let rows = criteria_semantics_rows(&mut sheet, false);
    let s = &mut sheet;

    // Metadata and dispatch come from the one generated catalogue: family 7 holds exactly these
    // entries, each eager, with Excel's arity, its parameter classes and its signature help.
    let expected: BTreeSet<(u32, String)> = CONDITIONALS
        .iter()
        .map(|(spelling, member)| ((7 << 16) | member, (*spelling).to_owned()))
        .collect();
    assert_eq!(
        conditional_family(),
        expected,
        "the conditional family in the catalogue"
    );
    let reference_class = Some(ParameterClass::Reference);
    let either = Some(ParameterClass::ValueOrReference);
    for (spelling, arity, classes) in [
        ("COUNTIF", (2, 2), vec![reference_class, either]),
        (
            "COUNTIFS",
            (2, 254),
            vec![reference_class, either, reference_class, either, either],
        ),
        (
            "SUMIF",
            (2, 3),
            vec![reference_class, either, reference_class],
        ),
        (
            "SUMIFS",
            (3, 255),
            vec![
                reference_class,
                reference_class,
                either,
                reference_class,
                either,
                either,
            ],
        ),
        (
            "AVERAGEIF",
            (2, 3),
            vec![reference_class, either, reference_class],
        ),
        (
            "AVERAGEIFS",
            (3, 255),
            vec![
                reference_class,
                reference_class,
                either,
                reference_class,
                either,
            ],
        ),
        (
            "MAXIFS",
            (3, 255),
            vec![
                reference_class,
                reference_class,
                either,
                reference_class,
                either,
            ],
        ),
        (
            "MINIFS",
            (3, 255),
            vec![
                reference_class,
                reference_class,
                either,
                reference_class,
                either,
            ],
        ),
    ] {
        let target = surface(Notation::Call, spelling);
        assert_eq!(
            target.argument_demand(),
            ArgumentDemand::Eager,
            "{spelling}"
        );
        let shape = target.arity();
        assert_eq!((shape.min, shape.max), arity, "{spelling}");
        for (index, class) in classes.iter().enumerate() {
            assert_eq!(
                target.parameter_class(index),
                *class,
                "{spelling} argument {index}"
            );
        }
        assert_eq!(target.parameter_class(shape.max), None, "{spelling}");
    }
    for (spelling, display) in [
        ("COUNTIF", "COUNTIF(range, criteria)"),
        ("SUMIF", "SUMIF(range, criteria, [sum_range])"),
        ("AVERAGEIF", "AVERAGEIF(range, criteria, [average_range])"),
        (
            "COUNTIFS",
            "COUNTIFS(criteria_range1, criteria1, [criteria_range2], [criteria2], ...)",
        ),
        (
            "SUMIFS",
            "SUMIFS(sum_range, criteria_range1, criteria1, [criteria_range2], [criteria2], ...)",
        ),
        (
            "MAXIFS",
            "MAXIFS(max_range, criteria_range1, criteria1, [criteria_range2], [criteria2], ...)",
        ),
    ] {
        assert_eq!(
            surface(Notation::Call, spelling).signature_help().display,
            display
        );
    }
    let manifest = manifest_ids(CONDITIONAL_RECORD);
    let listed: BTreeSet<(String, String)> = CONDITIONALS
        .iter()
        .map(|(spelling, member)| (format!("7.{member}"), (*spelling).to_owned()))
        .collect();
    assert_eq!(
        manifest, listed,
        "the per-ID manifest of migration/ports/cfunc-m3.5.json against the catalogue"
    );
    note!(
        "ACCEPT-NOTE: {CASE}: the catalogue's family 7 holds exactly the 8 entries {:?}, each eager; COUNTIF takes 2 arguments, SUMIF and AVERAGEIF 2 or 3, COUNTIFS 2 to 254 and the others 3 to 255; ranges are reference parameters and criteria values or references; the per-ID manifest of migration/ports/cfunc-m3.5.json lists the same IDs and spellings",
        CONDITIONALS.map(|(spelling, _)| spelling)
    );

    // Through the call interface alone. A range that is not a reference is refused naming it:
    // at admission for a declared reference parameter, by the kernel for a criteria range past
    // the declared ones; a count of arguments that does not form pairs is ArityMismatch.
    let range = reference(s, &["A1:A16"]).value;
    let one = CalcValue::number(1.0);
    let array = CalcValue::array(
        CalcArray::from_rows(vec![vec![CalcValue::number(1.0), CalcValue::number(2.0)]])
            .expect("a 1-by-2 array"),
    );
    let mut refused_calls = 0;
    for (spelling, values, fault_index) in [
        ("COUNTIF", vec![array.clone(), one.clone()], 0),
        ("SUMIF", vec![range.clone(), one.clone(), one.clone()], 2),
        ("AVERAGEIF", vec![one.clone(), one.clone()], 0),
        ("SUMIFS", vec![array.clone(), range.clone(), one.clone()], 0),
        (
            "COUNTIFS",
            vec![
                range.clone(),
                one.clone(),
                range.clone(),
                one.clone(),
                array.clone(),
                one.clone(),
            ],
            4,
        ),
        (
            "MAXIFS",
            vec![
                range.clone(),
                range.clone(),
                one.clone(),
                range.clone(),
                one.clone(),
                one.clone(),
                one.clone(),
            ],
            5,
        ),
    ] {
        let id = surface(Notation::Call, spelling).id();
        assert_eq!(
            conditional_call(spelling, values, &fast, s),
            Err(FunctionFault::ReferenceRequired {
                id,
                index: fault_index
            }),
            "{spelling} with a non-reference range at argument {fault_index}"
        );
        refused_calls += 1;
    }
    for (spelling, count) in [
        ("COUNTIFS", 3),
        ("SUMIFS", 4),
        ("AVERAGEIFS", 6),
        ("MINIFS", 254),
    ] {
        let id = surface(Notation::Call, spelling).id();
        // Ranges where the function takes them, criteria between them: COUNTIFS's pairs start
        // at argument 0, the others' behind the target at argument 1.
        let first_criterion = if spelling == "COUNTIFS" { 1 } else { 2 };
        let mut values = vec![range.clone(); count];
        for value in values.iter_mut().skip(first_criterion).step_by(2) {
            *value = one.clone();
        }
        assert_eq!(
            conditional_call(spelling, values, &fast, s),
            Err(FunctionFault::ArityMismatch {
                id,
                supplied: count
            }),
            "{spelling} with {count} arguments"
        );
        refused_calls += 1;
    }
    // A criterion that is an array or a reference of several cells lifts the function over it,
    // which is not modelled; so is a NaN criterion, which no formula writes. A blank given
    // directly reads as an empty cell does, 0.
    for criterion in [
        array.clone(),
        reference(s, &["A1:A2"]).value,
        reference(s, &["A1", "A2"]).value,
        CalcValue::number(f64::NAN),
    ] {
        let id = surface(Notation::Call, "COUNTIF").id();
        assert_eq!(
            conditional_call("COUNTIF", vec![range.clone(), criterion.clone()], &fast, s),
            Err(FunctionFault::NotImplemented {
                id,
                argument: Some(1)
            }),
            "COUNTIF with the criterion {criterion:?}"
        );
        refused_calls += 1;
    }
    assert_eq!(
        conditional_call("COUNTIF", vec![range.clone(), CalcValue::empty()], &fast, s)
            .map(|value| value.as_number()),
        Ok(Some(1.0)),
        "COUNTIF over A1:A16 with a direct blank criterion"
    );
    // A reference criterion is read first, through the reader: asked for A8, then A1:A16.
    s.reads.clear();
    let criterion_cell = reference(s, &["A8"]).value;
    let counted = conditional_call(
        "COUNTIF",
        vec![range.clone(), criterion_cell.clone()],
        &fast,
        s,
    );
    assert_eq!(counted.map(|value| value.as_number()), Ok(Some(1.0)));
    assert_eq!(s.reads, vec![s.area("A8"), s.area("A1:A16")]);
    // Without a reader, or over a refusing one, a call ends in ReferenceUnreadable naming the
    // argument it was reading: the criterion cell before the range.
    let mut unreadable = 0;
    for (spelling, values, index) in [
        ("COUNTIF", vec![range.clone(), one.clone()], 0),
        ("COUNTIF", vec![range.clone(), criterion_cell.clone()], 1),
        ("SUMIFS", vec![range.clone(), range.clone(), one.clone()], 1),
        ("SUMIF", vec![range.clone(), one.clone(), range.clone()], 0),
        ("MINIFS", vec![range.clone(), range.clone(), one.clone()], 1),
    ] {
        let target = surface(Notation::Call, spelling);
        let fault = Err(FunctionFault::ReferenceUnreadable {
            id: target.id(),
            index,
        });
        assert_eq!(
            run(target, &mut Recorded::new(values.clone()), &fast),
            fault,
            "{spelling} without a cell reader"
        );
        s.refuse = true;
        let refused = conditional_call(spelling, values, &fast, s);
        s.refuse = false;
        assert_eq!(refused, fault, "{spelling} over a refusing reader");
        unreadable += 2;
    }
    note!(
        "ACCEPT-NOTE: {CASE}: through the call interface alone: {refused_calls} calls refused a non-reference range (ReferenceRequired naming it), an argument count that does not form pairs (ArityMismatch), or a criterion that is an array, a reference of several cells or NaN (NotImplemented naming it); a direct blank criterion counts the one 0; COUNTIF(A1:A16,A8) asked the reader for A8 and then A1:A16; {unreadable} calls without a reader or over a refusing one ended in ReferenceUnreadable naming the argument being read"
    );

    // NumericMode reaches every arithmetic site. Under Exact, a call that compares two different
    // numbers, reads number text or adds is refused; one that compares only texts, logicals,
    // errors and blank cells, or chooses an extreme, computes the value of its Fast row.
    let exact = exact_mode();
    let mut exact_computed = 0;
    for (spelling, args) in [
        ("COUNTIF", vec![reference(s, &["A1:A16"]), txt("apple")]),
        ("COUNTIF", vec![reference(s, &["A1:A16"]), txt("a*b")]),
        ("COUNTIF", vec![reference(s, &["A1:A16"]), txt("")]),
        ("COUNTIF", vec![reference(s, &["A1:A16"]), logical(true)]),
        ("COUNTIF", vec![reference(s, &["A1:A16"]), txt("#N/A")]),
        (
            "MAXIFS",
            vec![
                reference(s, &["B1:B16"]),
                reference(s, &["A1:A16"]),
                txt("a*"),
            ],
        ),
        (
            "MINIFS",
            vec![
                reference(s, &["B1:B16"]),
                reference(s, &["A1:A16"]),
                txt("a*"),
            ],
        ),
    ] {
        let target = surface(Notation::Call, spelling);
        let texts: Vec<&str> = args.iter().map(|a| a.excel.as_str()).collect();
        let formula = target.oracle_formula(&texts).expect("a formula");
        let under_fast = row_value(&rows, &formula);
        let under_exact = conditional_call(
            spelling,
            args.iter().map(|a| a.value.clone()).collect(),
            &exact,
            s,
        );
        assert!(
            under_exact
                .as_ref()
                .is_ok_and(|value| identical(value, under_fast)),
            "{formula} under Exact gives {under_exact:?}, under Fast {under_fast:?}"
        );
        exact_computed += 1;
    }
    let mut exact_refused = 0;
    for (spelling, args) in [
        ("COUNTIF", vec![reference(s, &["A1:A16"]), num("5")]),
        ("COUNTIF", vec![reference(s, &["A1:A16"]), txt("5")]),
        (
            "SUMIF",
            vec![
                reference(s, &["A1:A16"]),
                txt("apple"),
                reference(s, &["B1:B16"]),
            ],
        ),
        ("AVERAGEIF", vec![reference(s, &["C1:C16"]), txt(">12")]),
        (
            "COUNTIFS",
            vec![
                reference(s, &["A1:A16"]),
                txt("a*"),
                reference(s, &["C1:C16"]),
                txt(">10"),
            ],
        ),
    ] {
        let target = surface(Notation::Call, spelling);
        let result = conditional_call(
            spelling,
            args.iter().map(|a| a.value.clone()).collect(),
            &exact,
            s,
        );
        assert_eq!(
            result,
            Err(FunctionFault::ExactUnavailable { id: target.id() }),
            "{spelling} under Exact"
        );
        exact_refused += 1;
    }
    note!(
        "ACCEPT-NOTE: {CASE}: under NumericMode::Exact {exact_computed} calls that compare only texts, logicals, errors and blank cells, or choose an extreme, computed the value of their Fast oracle row, and {exact_refused} calls that compare different numbers, read number text or add were refused with ExactUnavailable"
    );

    // A reader may also visit a blank cell, as Empty: over such a reader every call that counts
    // or matches blank cells gives the value of its Fast oracle row.
    let mut dense_calls = 0;
    for (spelling, args) in [
        ("COUNTIF", vec![reference(s, &["A1:A16"]), txt("<>apple")]),
        ("COUNTIF", vec![reference(s, &["A1:A16"]), txt("")]),
        ("COUNTIF", vec![reference(s, &["A1:A16"]), txt("=")]),
        ("COUNTIF", vec![reference(s, &["A1:A16"]), txt("<>")]),
        ("COUNTIF", vec![reference(s, &["A1:A16"]), txt("<>*")]),
        (
            "COUNTIF",
            vec![reference(s, &["A1:A16"]), reference(s, &["A8"])],
        ),
        (
            "SUMIF",
            vec![
                reference(s, &["A1:A16"]),
                txt(""),
                reference(s, &["B1:B16"]),
            ],
        ),
        (
            "COUNTIFS",
            vec![
                reference(s, &["A1:A16"]),
                txt("="),
                reference(s, &["C1:C16"]),
                txt(">0"),
            ],
        ),
    ] {
        let target = surface(Notation::Call, spelling);
        let texts: Vec<&str> = args.iter().map(|a| a.excel.as_str()).collect();
        let formula = target.oracle_formula(&texts).expect("a formula");
        let mut dense = DenseReader(s);
        let visited_blanks = conditional_call(
            spelling,
            args.iter().map(|a| a.value.clone()).collect(),
            &fast,
            &mut dense,
        );
        let under_fast = row_value(&rows, &formula);
        assert!(
            visited_blanks
                .as_ref()
                .is_ok_and(|value| identical(value, under_fast)),
            "{formula} over a reader that visits blank cells gives {visited_blanks:?}, the row {under_fast:?}"
        );
        dense_calls += 1;
    }
    note!(
        "ACCEPT-NOTE: {CASE}: {dense_calls} calls over a reader that also visits every blank cell as Empty gave the values of their Fast oracle rows"
    );

    // A reader that breaks the CellReader contract cannot supply the range, and the call ends in
    // ReferenceUnreadable naming it: a visit outside the area, cells out of row-major order, and
    // a cell visited twice. Each call first gives a value over the test sheet's reader.
    let breaches: [(&str, &str, &str, Breach, f64); 3] = [
        ("COUNTIF", "A1:A2", "apple", Breach::Outside("C1"), 1.0),
        ("COUNTIF", "A1:A2", "*", Breach::Reversed, 2.0),
        ("SUMIF", "C1", ">0", Breach::Twice, 1.0),
    ];
    for (spelling, area, criterion, breach, kept) in breaches {
        let target = surface(Notation::Call, spelling);
        let values = vec![reference(s, &[area]).value, text_value(criterion)];
        let honest = conditional_call(spelling, values.clone(), &fast, s);
        assert_eq!(
            honest
                .as_ref()
                .map(|value| value.as_number().map(f64::to_bits)),
            Ok(Some(kept.to_bits())),
            "{spelling}({area},{criterion:?}) over the test sheet's reader: {honest:?}"
        );
        let mut breaching = BreachingReader { sheet: s, breach };
        let broken = conditional_call(spelling, values, &fast, &mut breaching);
        assert_eq!(
            broken,
            Err(FunctionFault::ReferenceUnreadable {
                id: target.id(),
                index: 0
            }),
            "{spelling}({area}) over a reader that breaks the contract by {breach:?}"
        );
        note!(
            "ACCEPT-NOTE: {CASE}: {spelling}({area},\"{criterion}\") = {kept} over the test sheet's reader, and ReferenceUnreadable (argument 0) over a reader that breaks the CellReader contract by {breach:?}"
        );
    }

    assert_workbook_sheet(CONDITIONAL_WORKBOOK, CONDITIONAL_WORKBOOK_PATH, &rows, s);
    assert_conditional_workbook_generated(CASE);
    conditional_rows_note(&rows);
}

// ---------------------------------------------------------------------------------------------
// cfunc_m3_5__mismatched_shapes

/// The cells the shape rows read: A2:A5 and B3:B6 are the MAXIFS page's example of ranges of one
/// size at different rows; C2:C5 holds a second criterion aligned with A2:A5; row 1048576, the
/// last, holds a cell in each column, so a whole column's cells span the sheet.
fn mismatched_shapes_sheet() -> TestSheet {
    TestSheet::new(&[
        ("A2", "10", CalcValue::number(10.0)),
        ("A3", "1", CalcValue::number(1.0)),
        ("A4", "100", CalcValue::number(100.0)),
        ("A5", "1", CalcValue::number(1.0)),
        ("A1048576", "7", CalcValue::number(7.0)),
        ("B3", "text \"a\"", text_value("a")),
        ("B4", "text \"a\"", text_value("a")),
        ("B5", "text \"b\"", text_value("b")),
        ("B6", "text \"a\"", text_value("a")),
        ("B1048576", "text \"a\"", text_value("a")),
        ("C2", "text \"x\"", text_value("x")),
        ("C3", "text \"y\"", text_value("y")),
        ("C4", "text \"x\"", text_value("x")),
        ("C5", "text \"x\"", text_value("x")),
        ("C1048576", "text \"x\"", text_value("x")),
    ])
}

/// Runs a shape row and checks, by the test sheet's own instruments, that the call asked the
/// reader for exactly `reads`, in order, once each, and was visited only with their held cells;
/// then calls it again and counts the bytes this thread allocated during the call. Returns the
/// cells of the areas read, the cells visited and the bytes.
fn shape_row(
    rows: &mut Rows,
    s: &mut TestSheet,
    spelling: &'static str,
    args: &[Arg],
    reads: &[&str],
    expect: Expect,
    basis: Basis,
) -> (u64, usize, u64) {
    s.reads.clear();
    s.visited = 0;
    basis_row(
        "cfunc-m3.5",
        rows,
        s,
        named(spelling),
        args,
        expect.clone(),
        basis,
    );
    let areas: Vec<GridArea> = reads.iter().map(|text| s.area(text)).collect();
    let held: usize = areas
        .iter()
        .map(|area| {
            s.cells
                .keys()
                .filter(|(row, column)| {
                    area.contains(CellKey::new(s.id, *row, *column).expect("a held cell"))
                })
                .count()
        })
        .sum();
    let texts: Vec<&str> = args.iter().map(|a| a.excel.as_str()).collect();
    assert_eq!(
        (s.reads.as_slice(), s.visited),
        (areas.as_slice(), held),
        "{spelling}({}) asks for exactly these areas and is visited only with their held cells",
        texts.join(",")
    );
    let cells: u64 = areas.iter().map(|area| area.cell_count()).sum();
    let mut again = Recorded::new(args.iter().map(|a| a.value.clone()).collect());
    let target = surface(Notation::Call, spelling);
    let fast = NumericMode::Fast;
    let (result, bytes) = bytes_allocated_during(|| {
        black_box(invoke(
            target,
            &mut again,
            &mut EvalServices::with_cells(&fast, s),
            &mut FunctionCallScratch::new(),
        ))
    });
    assert!(
        expect.holds(target.id(), &result),
        "{spelling} again: {result:?}"
    );
    assert!(
        bytes < SPARSE_BYTES,
        "{spelling}({}) allocated {bytes} bytes over {cells} cells",
        texts.join(",")
    );
    (cells, held, bytes)
}

/// The rows of cfunc_m3_5__mismatched_shapes, run through calc_func::invoke on `s`, in the order
/// the conditional oracle workbook's sheet `mismatched_shapes` holds their formulas, each checked
/// by the test sheet's instruments ([`shape_row`]); with, for each row, what the reader was asked
/// for and visited and the bytes the call allocated.
fn mismatched_shapes_rows(s: &mut TestSheet, quiet: bool) -> (Rows, Vec<String>) {
    const CASE: &str = "cfunc_m3_5__mismatched_shapes";
    let mut rows = Rows::new(CASE, "mismatched_shapes", quiet);
    let r = &mut rows;
    use WorksheetErrorCode::Value;
    let mut measured = Vec::new();
    let mut measure = |r: &mut Rows,
                       s: &mut TestSheet,
                       spelling: &'static str,
                       args: Vec<Arg>,
                       reads: &[&str],
                       expect: Expect,
                       basis: Basis| {
        let texts: Vec<String> = args.iter().map(|a| a.excel.clone()).collect();
        let (cells, visited, bytes) = shape_row(r, s, spelling, &args, reads, expect, basis);
        measured.push(format!(
            "{spelling}({}): {} areas read [{}], {cells} cells, {visited} visited, {bytes} bytes allocated",
            texts.join(","),
            reads.len(),
            reads.join(", ")
        ));
    };
    let at = |s: &TestSheet, texts: &[&str]| reference(s, texts);

    // Ranges of one shape align by position, wherever they lie; ranges of different sizes or
    // shapes are #VALUE!, decided before any cell or criterion is read.
    measure(
        r,
        s,
        "MAXIFS",
        vec![at(s, &["A2:A5"]), at(s, &["B3:B6"]), txt("a")],
        &["B3:B6", "A2:A5"],
        N(10.0),
        Documented(
            "the MAXIFS page's example MAXIFS(A2:A5,B3:B6,\"a\") = 10: the ranges need the same size and shape, not the same rows",
        ),
    );
    measure(
        r,
        s,
        "MINIFS",
        vec![at(s, &["A2:A5"]), at(s, &["B3:B6"]), txt("a")],
        &["B3:B6", "A2:A5"],
        N(1.0),
        Documented(SAME_SHAPE),
    );
    measure(
        r,
        s,
        "MAXIFS",
        vec![at(s, &["A2:A5"]), at(s, &["B2:C6"]), txt("a")],
        &[],
        E(Value),
        Documented(
            "the MAXIFS page's example MAXIFS(A2:A5,B2:c6,\"a\") = #VALUE!: the ranges differ in size and shape",
        ),
    );
    measure(
        r,
        s,
        "MINIFS",
        vec![at(s, &["A2:A5"]), at(s, &["B3:C4"]), txt("a")],
        &[],
        E(Value),
        Documented(
            "4 rows by 1 column against 2 by 2: as many cells, another shape, #VALUE!, as the MINIFS page states: the size and shape of min_range and every criteria_range must be the same, otherwise #VALUE!",
        ),
    );
    measure(
        r,
        s,
        "SUMIFS",
        vec![at(s, &["A2:A5"]), at(s, &["B3:B6"]), txt("a")],
        &["B3:B6", "A2:A5"],
        N(12.0),
        Documented(SAME_SHAPE),
    );
    measure(
        r,
        s,
        "SUMIFS",
        vec![at(s, &["A2:A5"]), at(s, &["B3:B5"]), txt("a")],
        &[],
        E(Value),
        Choice(SHAPE_VALUE),
    );
    measure(
        r,
        s,
        "SUMIFS",
        vec![
            at(s, &["A2:A5"]),
            at(s, &["B3:B6"]),
            txt("a"),
            at(s, &["C2:C5"]),
            txt("x"),
        ],
        &["B3:B6", "C2:C5", "A2:A5"],
        N(11.0),
        Documented("a cell is added when its position meets every criterion: positions 1 and 4"),
    );
    measure(
        r,
        s,
        "SUMIFS",
        vec![
            at(s, &["A2:A5"]),
            at(s, &["B3:B6"]),
            txt("a"),
            at(s, &["C2:C4"]),
            txt("x"),
        ],
        &[],
        E(Value),
        Choice(SHAPE_VALUE),
    );
    measure(
        r,
        s,
        "SUMIFS",
        vec![at(s, &["A2:A5"]), at(s, &["B3:B5"]), txt("Q1")],
        &[],
        E(Value),
        Measured(
            "ranges of different sizes return #VALUE! beside the label criterion Q1, which is text (dc-e00t's rule); Excel's capture of cfunc-m3.5's oracle workbook holds #VALUE! in this row's cell (mismatched_shapes!D9), compared in cfunc_m3_5__excel_comparison",
        ),
    );
    measure(
        r,
        s,
        "COUNTIFS",
        vec![at(s, &["B3:B6"]), txt("a"), at(s, &["C2:C5"]), txt("x")],
        &["B3:B6", "C2:C5"],
        N(2.0),
        Documented(
            "the COUNTIFS page: each range has the same rows and columns as criteria_range1, and the cells are checked position by position",
        ),
    );
    measure(
        r,
        s,
        "COUNTIFS",
        vec![at(s, &["B3:B6"]), txt("a"), at(s, &["C2:C6"]), txt("x")],
        &[],
        E(Value),
        Choice(SHAPE_VALUE),
    );
    measure(
        r,
        s,
        "AVERAGEIFS",
        vec![at(s, &["A2:A5"]), at(s, &["B3:B6"]), txt("a")],
        &["B3:B6", "A2:A5"],
        N(4.0),
        Documented(SAME_SHAPE),
    );
    measure(
        r,
        s,
        "AVERAGEIFS",
        vec![
            at(s, &["A2:A5"]),
            at(s, &["B3:B6"]),
            txt("a"),
            at(s, &["C2:C3"]),
            txt("x"),
        ],
        &[],
        E(Value),
        Choice(
            "the AVERAGEIFS page: unlike AVERAGEIF, each criteria range has average_range's size and shape; the error is #VALUE!, as the MAXIFS page states for MAXIFS",
        ),
    );

    // SUMIF and AVERAGEIF shape their third argument from its top-left cell, inside the reference
    // the formula gives or past it (dc-b1l: the engine files the cells read past it as dynamic
    // prerequisites).
    measure(
        r,
        s,
        "SUMIF",
        vec![at(s, &["B3:B6"]), txt("a"), at(s, &["A2"])],
        &["B3:B6", "A2:A5"],
        N(12.0),
        Measured(SHAPED_PAST_MEASURED),
    );
    measure(
        r,
        s,
        "SUMIF",
        vec![at(s, &["B3:B6"]), txt("a"), at(s, &["A2:A3"])],
        &["B3:B6", "A2:A5"],
        N(12.0),
        Measured(SHAPED_PAST_MEASURED),
    );
    measure(
        r,
        s,
        "SUMIF",
        vec![at(s, &["B3:B6"]), txt("a"), at(s, &["A2:C9"])],
        &["B3:B6", "A2:A5"],
        N(12.0),
        Documented(
            "a larger sum_range is cut to the range's size from its top-left cell, as the SUMIF page's table shows B1:K5 used as B1:B5",
        ),
    );
    measure(
        r,
        s,
        "AVERAGEIF",
        vec![at(s, &["B3:B6"]), txt("a"), at(s, &["A2"])],
        &["B3:B6", "A2:A5"],
        N(4.0),
        Measured(SHAPED_PAST_MEASURED),
    );
    measure(
        r,
        s,
        "AVERAGEIF",
        vec![at(s, &["B3:B6"]), txt("b"), at(s, &["A2:B2"])],
        &["B3:B6", "A2:A5"],
        N(100.0),
        Measured(SHAPED_PAST_MEASURED),
    );
    measure(
        r,
        s,
        "SUMIF",
        vec![at(s, &["B3:C4"]), txt("a"), at(s, &["A2"])],
        &["B3:C4", "A2:B3"],
        N(11.0),
        Measured(SHAPED_PAST_MEASURED),
    );
    measure(
        r,
        s,
        "COUNTIFS",
        vec![at(s, &["B3:C4"]), txt("a"), at(s, &["A2:B3"]), txt(">5")],
        &["B3:C4", "A2:B3"],
        N(1.0),
        Documented("two-dimensional ranges align position by position in row-major order"),
    );
    measure(
        r,
        s,
        "SUMIF",
        vec![at(s, &["B3:B6"]), txt("a"), at(s, &["A1048575"])],
        &["B3:B6", "A1048575:A1048576"],
        N(7.0),
        Measured(
            "dc-b1l: the sum_range shaped from A1048575 to 4 rows is cut at the sheet's last row (Excel's capture of this row: 7)",
        ),
    );
    measure(
        r,
        s,
        "SUMIF",
        vec![at(s, &["B3:B4", "B6"]), txt("a")],
        &[],
        E(Value),
        Measured(
            "dc-b1l: a range of several areas is #VALUE!, at any range of the conditional aggregates, before the criterion is read (measured in Excel 16.0.20430 on 2026-10-25)",
        ),
    );

    // Whole columns: the reader visits the held cells only, and a count adds every blank row at
    // once.
    measure(
        r,
        s,
        "COUNTIF",
        vec![at(s, &["A:A"]), txt(">5")],
        &["A:A"],
        N(3.0),
        Documented("comparison operators work inside the criterion: 10, 100 and 7"),
    );
    measure(
        r,
        s,
        "COUNTIF",
        vec![at(s, &["B:B"]), txt("a")],
        &["B:B"],
        N(4.0),
        Documented("text criteria match the cells holding that text"),
    );
    measure(
        r,
        s,
        "COUNTIF",
        vec![at(s, &["B:B"]), txt("<>a")],
        &["B:B"],
        N(1_048_572.0),
        Choice(
            "<>a holds for every cell of the column but the four holding a, the blank cells included",
        ),
    );
    measure(
        r,
        s,
        "COUNTIF",
        vec![at(s, &["A:A"]), txt("")],
        &["A:A"],
        N(1_048_571.0),
        Documented(
            "\"\" matches a blank cell, as the SUMIF page's example SUMIF(A2:A7,\"\",C2:C7) shows: the column less its five numbers",
        ),
    );
    measure(
        r,
        s,
        "COUNTIF",
        vec![at(s, &["A:A"]), txt("<>1")],
        &["A:A"],
        N(1_048_574.0),
        Choice("<>1 holds for every cell of the column but A3 and A5, the blank cells included"),
    );
    measure(
        r,
        s,
        "SUMIF",
        vec![at(s, &["B:B"]), txt("a"), at(s, &["A:A"])],
        &["B:B", "A:A"],
        N(108.0),
        Documented("rows 3, 4, 6 and 1048576 meet the criterion; A6 is blank: 1 + 100 + 7"),
    );
    measure(
        r,
        s,
        "SUMIF",
        vec![at(s, &["B:B"]), txt("a"), at(s, &["A1"])],
        &["B:B", "A:A"],
        N(108.0),
        Measured(SHAPED_PAST_MEASURED),
    );
    measure(
        r,
        s,
        "SUMIFS",
        vec![
            at(s, &["A:A"]),
            at(s, &["B:B"]),
            txt("a"),
            at(s, &["C:C"]),
            txt("x"),
        ],
        &["B:B", "C:C", "A:A"],
        N(107.0),
        Documented("rows 4 and 1048576 meet both criteria: 100 + 7"),
    );
    measure(
        r,
        s,
        "COUNTIFS",
        vec![at(s, &["A:A"]), txt(">0"), at(s, &["C:C"]), txt("<>y")],
        &["A:A", "C:C"],
        N(4.0),
        Choice("rows 2, 4, 5 and 1048576: a blank row of A does not meet >0"),
    );
    measure(
        r,
        s,
        "COUNTIFS",
        vec![at(s, &["B:B"]), txt("<>a"), at(s, &["C:C"]), txt("<>y")],
        &["B:B", "C:C"],
        N(1_048_572.0),
        Choice(
            "rows 2 and 5 of the six held rows, and the 1048570 rows blank in both columns, which meet both <> criteria",
        ),
    );
    measure(
        r,
        s,
        "AVERAGEIFS",
        vec![at(s, &["A:A"]), at(s, &["B:B"]), txt("a")],
        &["B:B", "A:A"],
        N(36.0),
        Documented(
            "the average of the cells whose row meets the criterion; the blank A6 is skipped: (1 + 100 + 7) / 3",
        ),
    );
    measure(
        r,
        s,
        "MAXIFS",
        vec![at(s, &["A:A"]), at(s, &["B:B"]), txt("a")],
        &["B:B", "A:A"],
        N(100.0),
        Documented("the largest cell of max_range whose row meets the criterion"),
    );
    measure(
        r,
        s,
        "MINIFS",
        vec![at(s, &["A:A"]), at(s, &["C:C"]), txt("x")],
        &["C:C", "A:A"],
        N(1.0),
        Documented(
            "the smallest cell of min_range whose row meets the criterion: 10, 100, 1 and 7",
        ),
    );
    measure(
        r,
        s,
        "SUMIFS",
        vec![at(s, &["A:A"]), at(s, &["B1:B1048575"]), txt("a")],
        &[],
        E(Value),
        Choice(SHAPE_VALUE),
    );
    measure(
        r,
        s,
        "COUNTIFS",
        vec![at(s, &["A:A"]), txt(">0"), at(s, &["B:C"]), txt("a")],
        &[],
        E(Value),
        Choice(SHAPE_VALUE),
    );

    // Shaped third arguments inside the reference the formula gives.
    measure(
        r,
        s,
        "SUMIF",
        vec![at(s, &["B3:C4"]), txt("a"), at(s, &["A2:C9"])],
        &["B3:C4", "A2:B3"],
        N(11.0),
        Documented(
            "a two-dimensional range takes a sum_range of its rows and columns from the top-left cell, a larger one cut, as the SUMIF page's table shows B1:K5 used as B1:B5: A2:B3 aligns with B3:C4, and B3 and B4 hold a: 10 + 1",
        ),
    );
    measure(
        r,
        s,
        "AVERAGEIF",
        vec![at(s, &["B3:B6"]), txt("a"), at(s, &["A2:B5"])],
        &["B3:B6", "A2:A5"],
        N(4.0),
        Documented(TOP_LEFT),
    );
    (rows, measured)
}

#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test)]
fn cfunc_m3_5__mismatched_shapes() {
    const CASE: &str = "cfunc_m3_5__mismatched_shapes";
    let mut sheet = mismatched_shapes_sheet();
    note!(
        "ACCEPT-NOTE: {CASE}: the sheet holds {}; every other cell is blank",
        sheet.described()
    );
    let s = &mut sheet;
    let (rows, measured) = mismatched_shapes_rows(s, false);
    for line in &measured {
        note!("ACCEPT-NOTE: {CASE}: instrumented reader: {line}");
    }

    // The whole sheet, through the call interface alone: 2^34 cells, of which the reader visits
    // the 15 held, once per range.
    let whole = CalcValue::reference(ReferenceLike::area(GridArea::full_sheet(s.id)));
    let fast = NumericMode::Fast;
    for (spelling, values, expected, areas) in [
        (
            "COUNTIF",
            vec![whole.clone(), text_value("<>a")],
            17_179_869_180.0_f64,
            1,
        ),
        (
            "COUNTIFS",
            vec![
                whole.clone(),
                text_value("<>a"),
                whole.clone(),
                text_value("<>b"),
            ],
            17_179_869_179.0,
            2,
        ),
        // The target is the criteria range itself, read once.
        (
            "SUMIFS",
            vec![whole.clone(), whole.clone(), text_value("<>a")],
            119.0,
            1,
        ),
    ] {
        s.reads.clear();
        s.visited = 0;
        let target = surface(Notation::Call, spelling);
        let mut given = Recorded::new(values);
        let (result, bytes) = bytes_allocated_during(|| {
            black_box(invoke(
                target,
                &mut given,
                &mut EvalServices::with_cells(&fast, s),
                &mut FunctionCallScratch::new(),
            ))
        });
        assert_eq!(
            result
                .as_ref()
                .map(|value| value.as_number().map(f64::to_bits)),
            Ok(Some(expected.to_bits())),
            "{spelling} of the whole sheet: {result:?}"
        );
        let full = GridArea::full_sheet(s.id);
        assert_eq!(
            (s.reads.clone(), s.visited),
            (vec![full; areas], 15 * areas),
            "{spelling} of the whole sheet"
        );
        assert!(bytes < SPARSE_BYTES, "{spelling} allocated {bytes} bytes");
        note!(
            "ACCEPT-NOTE: {CASE}: {spelling} over A1:XFD1048576 through calc_func::invoke = {expected} over {} cells per range; the reader was asked for {areas} areas and visited {} cells; {bytes} bytes allocated",
            full.cell_count(),
            15 * areas
        );
    }

    assert_workbook_sheet(CONDITIONAL_WORKBOOK, CONDITIONAL_WORKBOOK_PATH, &rows, s);
    conditional_rows_note(&rows);
}

// ---------------------------------------------------------------------------------------------
// cfunc_m3_5__excel_comparison

/// The Excel comparison of the conditional rows: `run`, the rows of both sheets of the conditional
/// oracle workbook, published under that workbook and judged against `capture` by
/// calc_verdict::judge. Returns each sheet's comparison, in `run`'s order; refused exactly when
/// judge refuses.
fn conditional_comparison(
    capture: &CaptureRecord,
    run: &[&Rows],
) -> Result<Vec<Comparison>, JudgeError> {
    let (results, evaluations) = published_in(CONDITIONAL_WORKBOOK, CONDITIONAL_CAPTURE_CASE, run);
    run.iter()
        .map(|rows| judged(capture, &results, &evaluations, rows))
        .collect()
}

#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test)]
fn cfunc_m3_5__excel_comparison() {
    const CASE: &str = "cfunc_m3_5__excel_comparison";
    let mut criteria_sheet = criteria_semantics_sheet();
    let criteria = criteria_semantics_rows(&mut criteria_sheet, true);
    let mut shapes_sheet = mismatched_shapes_sheet();
    let (shapes, _) = mismatched_shapes_rows(&mut shapes_sheet, true);
    let run = [&criteria, &shapes];
    let all: usize = run.iter().map(|rows| rows.oracle.len()).sum();
    let refused: usize = run.iter().map(|rows| rows.refused()).sum();
    let computed = all - refused;
    for rows in run {
        assert!(
            rows.oracle
                .iter()
                .all(|row| row.owner.is_some() == row.outcome.is_err()),
            "{}: a refused row names its owner, a computed row none",
            rows.sheet
        );
    }

    // The comparison itself, on capture records built here in memory, which carry no Excel
    // evidence: a capture that observes each computed row's own value matches every computed row
    // and counts every refused row not modelled with its owner, and a capture whose observation
    // of one computed row differs in its last bit is exactly one problem, at that row's cell.
    let agreeing_capture = capture_in(
        CONDITIONAL_WORKBOOK,
        CONDITIONAL_CAPTURE_CASE,
        &run,
        agreeing,
    );
    let comparisons = conditional_comparison(&agreeing_capture, &run)
        .unwrap_or_else(|refusal| panic!("judge refused the in-memory capture: {refusal}"));
    for (rows, comparison) in run.iter().zip(&comparisons) {
        assert_eq!(comparison.problems, Vec::<String>::new(), "{}", rows.sheet);
    }
    let report = &comparisons[0].report;
    let counts = &report.counts;
    assert_eq!(
        (
            counts.denominator,
            counts.calculated,
            counts.matched,
            counts.not_modeled,
            counts.mismatched + counts.not_run + counts.unproven + counts.stale,
            counts.evaluator_calls
        ),
        (
            all as u64,
            computed as u64,
            computed as u64,
            refused as u64,
            0,
            all as u64
        ),
        "{}",
        report.summary()
    );
    let changed = criteria
        .oracle
        .iter()
        .find(|row| {
            row.outcome
                .as_ref()
                .is_ok_and(|value| value.as_number().is_some_and(|n| n != 0.0))
        })
        .expect("a computed row with a nonzero number")
        .row;
    let one_bit = capture_in(
        CONDITIONAL_WORKBOOK,
        CONDITIONAL_CAPTURE_CASE,
        &run,
        |sheet, row| {
            if sheet == criteria.sheet && row.row == changed {
                let n = row.outcome.as_ref().ok()?.as_number()?;
                Some(CalcValue::number(f64::from_bits(n.to_bits() ^ 1)))
            } else {
                agreeing(sheet, row)
            }
        },
    );
    let problems: Vec<String> = conditional_comparison(&one_bit, &run)
        .unwrap_or_else(|refusal| panic!("judge refused the in-memory capture: {refusal}"))
        .into_iter()
        .flat_map(|comparison| comparison.problems)
        .collect();
    let cell = format!("{}!{}", criteria.sheet, encode_a1(changed, FORMULA_COLUMN));
    assert!(
        problems.len() == 1 && problems[0].starts_with(&format!("{cell} ")),
        "a capture changed in the last bit at {cell} gives {problems:?}"
    );
    // A refused row that names cfunc-m3.5 itself as its owner is a problem, though the capture
    // agrees with it: this bead's refusals are not modelled elsewhere.
    let mut self_owned = criteria_semantics_rows(&mut criteria_semantics_sheet(), true);
    let row = self_owned
        .oracle
        .iter_mut()
        .find(|row| row.outcome.is_err())
        .expect("a refused row");
    row.owner = Some(Owner {
        bead: "cfunc-m3.5",
        behaviour: "a refusal of the bead under test",
    });
    let self_cell = format!(
        "{}!{}",
        self_owned.sheet,
        encode_a1(row.row, FORMULA_COLUMN)
    );
    let probed = [&self_owned, &shapes];
    let problems: Vec<String> = conditional_comparison(
        &capture_in(
            CONDITIONAL_WORKBOOK,
            CONDITIONAL_CAPTURE_CASE,
            &probed,
            agreeing,
        ),
        &probed,
    )
    .unwrap_or_else(|refusal| panic!("judge refused the in-memory capture: {refusal}"))
    .into_iter()
    .flat_map(|comparison| comparison.problems)
    .collect();
    assert!(
        problems.len() == 1
            && problems[0].starts_with(&format!("{self_cell} "))
            && problems[0].contains("names its own bead cfunc-m3.5"),
        "a row owned by cfunc-m3.5 at {self_cell} gives {problems:?}"
    );
    note!(
        "ACCEPT-NOTE: {CASE}: on in-memory captures of {CONDITIONAL_WORKBOOK_PATH} (no Excel evidence; {all} rows: {computed} computed, {refused} refused): calc_verdict::judge's report through the comparison matches every computed row of the agreeing capture and counts every refused row not modeled with its owner, none of which is cfunc-m3.5; a capture changed in the last bit at {cell} is exactly one problem, at that cell; the refused row at {self_cell} given cfunc-m3.5 as its owner is exactly one problem, at that cell"
    );

    // Excel's capture of the workbook.
    if let Some(missing) = MissingCapture::recognize(CONDITIONAL_CAPTURE) {
        assert_eq!(
            (
                missing.case.as_str(),
                missing.fixture.as_str(),
                missing.command.as_str()
            ),
            (
                CONDITIONAL_CAPTURE_CASE,
                CONDITIONAL_WORKBOOK_PATH,
                CONDITIONAL_CAPTURE_COMMAND
            ),
            "the absent-capture marker names this capture"
        );
        note!("ACCEPT-NOT-RUN: {CASE}: {missing}");
        return;
    }
    let capture =
        CaptureRecord::decode(CONDITIONAL_CAPTURE).expect("the committed capture decodes");
    assert_eq!(capture.case, CONDITIONAL_CAPTURE_CASE);
    assert_eq!(capture.acquisition, Acquisition::LiveCapture);
    let session = capture
        .session
        .as_ref()
        .expect("the committed capture is a version 2 record");
    let workbook = format!("sha256:{}", sha256_hex(CONDITIONAL_WORKBOOK));
    if capture.source != workbook {
        note!(
            "ACCEPT-NOT-RUN: {CASE}: awaiting capture: the committed capture is of {}, and the oracle workbook as committed is {workbook} (cargo xtask capture --pending takes it)",
            capture.source
        );
        return;
    }
    let isolation: Vec<_> = session.failures_at(STAGE_ISOLATION).collect();
    assert!(
        isolation.is_empty(),
        "the committed capture records isolation failures: {isolation:?}"
    );
    let environment = capture
        .environment
        .as_ref()
        .expect("the capture records Excel's environment");
    let comparisons = conditional_comparison(&capture, &run)
        .unwrap_or_else(|refusal| panic!("calc_verdict::judge refuses the capture: {refusal}"));
    for (rows, comparison) in run.iter().zip(&comparisons) {
        assert!(
            comparison.problems.is_empty(),
            "{} problems on sheet {} of the verdict on Excel {}'s capture ({}):\n{}",
            comparison.problems.len(),
            rows.sheet,
            environment.target.excel_build(),
            comparison.report.summary(),
            comparison.problems.join("\n")
        );
        note!(
            "ACCEPT-NOTE: {CASE}: Excel comparison through calc_verdict::judge against Excel {}'s capture of {CONDITIONAL_WORKBOOK_PATH}: sheet {}: {} (every computed row matched, every refused row not modeled with its owner); the whole workbook: {}",
            environment.target.excel_build(),
            rows.sheet,
            sheet_verdicts(&comparison.report, rows),
            comparison.report.summary()
        );
    }
}

// ---------------------------------------------------------------------------------------------
// dc-q9rb: a number's text when the & operator reads the number as text, through
// calc_func::invoke of & on its catalogue target.
//
// Each row runs one & call with a number among its operands. A row that computes asserts the text
// it gives against an expected text; a row that calc_func refuses asserts the refusal and prints
// the text Excel gives. Every expected text and every text Excel gives here is Excel's raw value
// for the row's cell in the committed capture of the oracle workbook ([`CAPTURED`]), and
// dc_q9rb__excel_comparison asserts each against the observation calc_verdict::judge's report
// carries for that cell. The same call under NumericMode::Exact has the same outcome: the
// conversion rounds decimal digits, exactly, and computes no binary64 result.
//
// What calc_func refuses, as not modelled: a number within a relative 2^-59 of a tie at its 16th
// significant digit, an exact tie included, when the side of the tie decides its text. Near such a
// tie Excel's own rounding departs from the exact one in both directions with no rule found: it
// rounds the exact ties of rows D5, D11, D12, D61, D72 and D73 toward zero and those of rows D77 to
// D80 away from zero, and the near ties of rows D81 and D82 against their exact value (the capture
// records formula cells only, so it cannot tell that apart from Excel reading the A-cell text as
// another binary64; both rows are refused). The rows that show it name dc-uar as the owner of that
// behaviour, and the Excel comparison publishes them as not modelled with that owner. Every other
// number gives its text (row D84 lies that near a tie, but its 14-digit text is the same on either
// side of it).
//
// Excel holds no -0 and no subnormal number: a cell written as one reads back as +0, and Excel
// stores the literal -0 in a formula as 0. A row whose operand is one of them gives calc_func an
// operand that Excel's own formula did not have, and its note says so.
//
// `tests/fixtures/dc_q9rb/oracle.xlsx`, written by the committed generator
// `tests/fixtures/make-fixtures.ps1`, holds every row's formula in column D of its sheet, in the
// order the case runs its rows, beside the cells the rows read. `cargo xtask capture --fixture
// crates/calc-func/tests/fixtures/dc_q9rb/oracle.xlsx --case dc_q9rb__excel_comparison --out
// crates/calc-func/tests/captures/dc_q9rb/oracle.capture.json` wrote Excel's capture of that
// workbook, and the comparison case dc_q9rb__excel_comparison publishes the rows' results and
// their recorded invoke calls to calc_verdict::judge against it.

/// The oracle workbook of dc-q9rb, as committed.
const NUMBER_TEXT_WORKBOOK: &[u8] = include_bytes!("fixtures/dc_q9rb/oracle.xlsx");
const NUMBER_TEXT_WORKBOOK_PATH: &str = "crates/calc-func/tests/fixtures/dc_q9rb/oracle.xlsx";
/// Excel's capture of that workbook.
const NUMBER_TEXT_CAPTURE: &[u8] = include_bytes!("captures/dc_q9rb/oracle.capture.json");
/// The case the capture is recorded under.
const NUMBER_TEXT_CAPTURE_CASE: &str = "dc_q9rb__excel_comparison";

/// How every expected text and every text Excel gives in the dc-q9rb rows was obtained. The
/// comparison case asserts the capture's locale and separators as this states them, and reports
/// the Excel build and COM add-ins the capture records.
const CAPTURED: &str = "Excel's raw value for this cell in the committed capture crates/calc-func/tests/captures/dc_q9rb/oracle.capture.json, which dc_q9rb__excel_comparison asserts through calc_verdict::judge's report (Excel on the owner's Windows machine, whose build and COM add-ins the capture records, under Windows locale en-ZA, with '.' as decimal separator and a space as thousands separator; en-US itself was not measured)";
/// The capture's settings that [`CAPTURED`] states.
const CAPTURED_SETTINGS: [(&str, &str); 3] = [
    ("user_culture", "en-ZA"),
    ("decimal_separator", "."),
    ("thousands_separator", " "),
];

/// The behaviour the refused dc-q9rb rows show.
const NEAR_TIE: Owner = Owner {
    bead: "dc-uar",
    behaviour: "a number's text within a relative 2^-59 of a tie at its 16th significant digit",
};

/// The numbers the rows read from cells, each in column A of its row's own line, as the generator
/// writes them: the shortest decimal that reads back as the number.
const NUMBER_TEXT_CELLS: [(&str, &str); 58] = [
    ("A1", "1"),
    ("A2", "123456789012345"),
    ("A3", "999999999999999"),
    ("A4", "999999999999999.4"),
    ("A5", "999999999999999.5"),
    ("A6", "999999999999999.9"),
    ("A8", "1000000000000001"),
    ("A9", "1000000000000000.5"),
    ("A10", "1234567890123456"),
    ("A11", "1234567890123455"),
    ("A12", "1234567890123465"),
    ("A13", "9007199254740992"),
    ("A15", "1E+19"),
    ("A16", "99999999999999900000"),
    ("A17", "9.999999999999999E+19"),
    ("A19", "1.2345678901234568E+20"),
    ("A21", "9.999999999999999E+20"),
    ("A22", "1.2345678901234568E+21"),
    ("A31", "0.000123456789012345"),
    ("A32", "1.23456789012345E-05"),
    ("A33", "1.2345678901234E-06"),
    ("A38", "0.30000000000000004"),
    ("A42", "0.9999999999999999"),
    ("A44", "-0.5"),
    ("A45", "-1000000000000000"),
    ("A46", "-1.23456789012345E-05"),
    ("A47", "-1.7976931348623157E+308"),
    ("A49", "-0"),
    ("A50", "1.7976931348623157E+308"),
    ("A51", "2.2250738585072014E-308"),
    ("A52", "2.225073858507201E-308"),
    ("A53", "5E-324"),
    ("A54", "-5E-324"),
    ("A55", "1.23456789012345E+98"),
    ("A56", "1.23456789012345E+99"),
    ("A57", "1.23456789012345E+100"),
    ("A58", "1.23456789012345E-99"),
    ("A60", "1.8055722654199485E-306"),
    ("A61", "7200547821627.375"),
    ("A67", "-1E+19"),
    ("A68", "-99999999999999900000"),
    ("A69", "-9.999999999999999E+19"),
    ("A70", "-0.000123456789012345"),
    ("A72", "-1234567890123455"),
    ("A73", "-7200547821627.375"),
    ("A74", "-1.8055722654199485E-306"),
    ("A75", "9.99999999999995E-99"),
    ("A76", "-9.99999999999995E-99"),
    ("A77", "521310271410497.5"),
    ("A78", "5758361573115.375"),
    ("A79", "-6278515587045.875"),
    ("A80", "543212677140.0625"),
    ("A81", "4037429.935952025"),
    ("A82", "5.248519552626765E-06"),
    ("A83", "-0.9198894815859745"),
    ("A84", "-7.302438853328905E+163"),
    ("A85", "23.61461297829835"),
    ("A86", "80216.89735826345"),
];

/// The test sheet of the dc-q9rb rows: [`NUMBER_TEXT_CELLS`], each read as Rust reads the decimal,
/// which is the binary64 nearest it.
fn number_text_sheet() -> TestSheet {
    let cells: Vec<(&str, &str, CalcValue)> = NUMBER_TEXT_CELLS
        .iter()
        .map(|&(address, written)| {
            let value: f64 = written.parse().expect("a decimal");
            (address, written, CalcValue::number(value))
        })
        .collect();
    TestSheet::new(&cells)
}

/// What a dc-q9rb row expects of calc_func::invoke.
#[derive(Clone, Copy)]
enum TextOutcome {
    /// This text, which is the text Excel gives.
    Gives(&'static str),
    /// A number at or near a 16th-digit tie that Excel rounds up (dc-uar): calc_func gives the
    /// first text, the 15 digits below the tie, and Excel the second.
    Departs(&'static str, &'static str),
}

/// The dc-q9rb rows, with the text Excel gives for each, in row order.
struct TextRows {
    rows: Rows,
    excel: Vec<&'static str>,
    /// The rows whose text departs from Excel's ([`TextOutcome::Departs`]).
    departs: Vec<u32>,
}

/// Runs one dc-q9rb row: & on `args` through calc_func::invoke under Fast, asserted to have
/// `outcome`, and the same call under Exact, asserted to have the same outcome. Records the call
/// and the row, with the next cell of column D (a refused row owned by [`NEAR_TIE`]), and the text
/// Excel gives, and prints the row with how its texts were obtained and the row's `note`, if any.
fn number_text_check(
    text_rows: &mut TextRows,
    sheet: &mut TestSheet,
    args: &[Arg],
    outcome: TextOutcome,
    note: Option<&'static str>,
) {
    let rows = &mut text_rows.rows;
    assert!(
        args.iter().any(|a| a.value.as_number().is_some()),
        "a row of dc-q9rb reads a number as text"
    );
    let (expect, owner) = match outcome {
        TextOutcome::Gives(text) => (t(text), None),
        TextOutcome::Departs(ours, _) => (t(ours), None),
    };
    let (formula, _, result, tick) =
        call_and_assert(sheet, &NumericMode::Fast, AMPERSAND, args, &expect);
    let exact = exact_mode();
    let mut given = Recorded::new(args.iter().map(|a| a.value.clone()).collect());
    let under_exact = invoke(
        surface(AMPERSAND.0, AMPERSAND.1),
        &mut given,
        &mut EvalServices::with_cells(&exact, sheet),
        &mut FunctionCallScratch::new(),
    );
    assert!(
        match (&under_exact, &result) {
            (Ok(e), Ok(f)) => identical(e, f),
            (Err(e), Err(f)) => e == f,
            _ => false,
        },
        "{formula} under Exact gives {under_exact:?}, under Fast {result:?}"
    );
    let row = u32::try_from(rows.oracle.len() + 1).expect("a row number");
    if !rows.quiet {
        let reads: Vec<&str> = args
            .iter()
            .flat_map(|a| a.reads.iter().map(String::as_str))
            .collect();
        let cells = if reads.is_empty() {
            String::new()
        } else {
            format!(" with {}", reads.join(", "))
        };
        let note = note.map(|note| format!("; {note}")).unwrap_or_default();
        let shown = match outcome {
            TextOutcome::Gives(text) => {
                format!("text {text:?}, the same under Exact (expected text as captured)")
            }
            TextOutcome::Departs(ours, excel) => format!(
                "text {ours:?}, the same under Exact, the 15 digits below the tie; Excel gives {excel:?} (as captured), a departure no rule found explains (dc-uar)"
            ),
        };
        note!(
            "ACCEPT-NOTE: {}: oracle {}!{} {formula}{cells} -> {shown}{note}",
            rows.case,
            rows.sheet,
            encode_a1(row, FORMULA_COLUMN)
        );
    }
    rows.calls.push(Call { row, tick });
    rows.oracle.push(OracleRow {
        row,
        formula,
        outcome: result,
        choice: None,
        owner,
        tick,
    });
    text_rows.excel.push(match outcome {
        TextOutcome::Gives(text) | TextOutcome::Departs(_, text) => text,
    });
    if let TextOutcome::Departs(..) = outcome {
        text_rows.departs.push(row);
    }
}

/// Runs a dc-q9rb row that computes: `nt!(rows, sheet, [args], text)`, its expected text the
/// captured one, or with a fifth argument, a note the row prints.
macro_rules! nt {
    ($rows:expr, $sheet:expr, [$($arg:expr),* $(,)?], $text:expr $(, $note:expr)?) => {{
        let args = [$($arg),*];
        let note: Option<&'static str> = None $(.or(Some($note)))?;
        number_text_check($rows, $sheet, &args, TextOutcome::Gives($text), note)
    }};
}

/// Runs a dc-q9rb row at or near a 16th-digit tie that Excel rounds up:
/// `departs!(rows, sheet, [args], ours, excel, note)`, `ours` the text calc_func gives and `excel`
/// the text Excel gives, as captured.
macro_rules! departs {
    ($rows:expr, $sheet:expr, [$($arg:expr),* $(,)?], $ours:expr, $excel:expr, $note:expr) => {{
        let args = [$($arg),*];
        number_text_check($rows, $sheet, &args, TextOutcome::Departs($ours, $excel), Some($note))
    }};
}

/// The note of a row whose operand is a cell written as -0 or as a subnormal number.
const READS_BACK_AS_ZERO: &str = "Excel's operand was +0: the package Excel saved during the capture (sha256:744760a3..., the output package the capture's session records) holds this cell as 0, while calc_func is given the number as written";
/// The note of the row of the literal -0.
const STORED_AS_ZERO: &str = "Excel's operand was +0: Excel stores the literal -0 as 0, and the capture records this formula as =0&\"\", while calc_func is given -0; that -0's text is 0 rests on Excel holding no -0, not on a measured -0 operand";
/// The note of a row whose number operand the formula computes.
const COMPUTED_OPERAND: &str = "its number operand computed through calc_func::invoke of the formula's arithmetic operator under Fast, the binary64 the same expression gives in Rust; the comparison records the & call";
/// The note of a row that is an exact tie at the 16th digit which Excel rounds toward zero.
const TIE_TOWARD_ZERO: &str =
    "an exact tie at the 16th significant digit, which Excel rounds toward zero";
/// The note of a row that is an exact tie at the 16th digit which Excel rounds away from zero.
const TIE_AWAY: &str =
    "an exact tie at the 16th significant digit, which Excel rounds away from zero";

/// An operand of a dc-q9rb row that a formula computes, `excel` as the formula writes it: `op` on
/// the numbers `a` and `b` through calc_func::invoke under Fast, asserted to be `expected`.
fn computed(op: (Notation, &str), excel: &str, a: f64, b: f64, expected: f64) -> Arg {
    let mut given = Recorded::new(vec![CalcValue::number(a), CalcValue::number(b)]);
    let value = run(surface(op.0, op.1), &mut given, &NumericMode::Fast)
        .unwrap_or_else(|fault| panic!("{excel} through calc_func::invoke: {fault:?}"));
    assert_eq!(
        value.as_number().map(f64::to_bits),
        Some(expected.to_bits()),
        "{excel} through calc_func::invoke gives {value:?}"
    );
    arg(excel, value)
}

/// The rows of dc_q9rb__number_text_for_concatenation, run through calc_func::invoke on `s`, in
/// the order the oracle workbook's sheet holds their formulas.
fn number_text_rows(s: &mut TestSheet, quiet: bool) -> TextRows {
    const CASE: &str = "dc_q9rb__number_text_for_concatenation";
    let mut rows = TextRows {
        departs: Vec::new(),
        rows: Rows::new(CASE, "number_text_for_concatenation", quiet),
        excel: Vec::new(),
    };
    let r = &mut rows;
    let empty = || txt("");

    // Integers up to and past 15 significant digits, and the boundaries around 1E+15. The 16th
    // digit rounds the 15th; an exact tie there (999999999999999.5, 1234567890123455,
    // 1234567890123465) is refused.
    nt!(r, s, [cell(s, "A1"), empty()], "1");
    nt!(r, s, [cell(s, "A2"), empty()], "123456789012345");
    nt!(r, s, [cell(s, "A3"), empty()], "999999999999999");
    nt!(r, s, [cell(s, "A4"), empty()], "999999999999999");
    nt!(
        r,
        s,
        [cell(s, "A5"), empty()],
        "999999999999999",
        TIE_TOWARD_ZERO
    );
    nt!(r, s, [cell(s, "A6"), empty()], "1000000000000000");
    nt!(r, s, [num("1E+15"), empty()], "1000000000000000");
    nt!(r, s, [cell(s, "A8"), empty()], "1000000000000000");
    nt!(r, s, [cell(s, "A9"), empty()], "1000000000000000");
    nt!(r, s, [cell(s, "A10"), empty()], "1234567890123460");
    nt!(
        r,
        s,
        [cell(s, "A11"), empty()],
        "1234567890123450",
        TIE_TOWARD_ZERO
    );
    nt!(
        r,
        s,
        [cell(s, "A12"), empty()],
        "1234567890123460",
        TIE_TOWARD_ZERO
    );
    nt!(r, s, [cell(s, "A13"), empty()], "9007199254740990");
    nt!(r, s, [txt("x"), num("1E+16")], "x10000000000000000");

    // Where the text switches to E notation: past 20 characters, so between 99999999999999900000
    // and 1E+20; and 1E+21 and beyond.
    nt!(r, s, [cell(s, "A15"), empty()], "10000000000000000000");
    nt!(r, s, [cell(s, "A16"), empty()], "99999999999999900000");
    nt!(r, s, [cell(s, "A17"), empty()], "1E+20");
    nt!(r, s, [num("1E+20"), empty()], "1E+20");
    nt!(r, s, [cell(s, "A19"), empty()], "1.23456789012346E+20");
    nt!(r, s, [num("1E+21"), empty()], "1E+21");
    nt!(r, s, [cell(s, "A21"), empty()], "1E+21");
    nt!(r, s, [cell(s, "A22"), empty()], "1.23456789012346E+21");
    nt!(r, s, [num("1E+22"), empty()], "1E+22");

    // Small numbers from 1E-4 to 1E-10 have no exponent; past 20 characters they do.
    nt!(r, s, [num("0.0001"), empty()], "0.0001");
    nt!(r, s, [num("0.00001"), empty()], "0.00001");
    nt!(r, s, [num("0.000001"), empty()], "0.000001");
    nt!(r, s, [num("1E-7"), empty()], "0.0000001");
    nt!(r, s, [num("1E-8"), empty()], "0.00000001");
    nt!(r, s, [num("1E-9"), empty()], "0.000000001");
    nt!(r, s, [num("1E-10"), empty()], "0.0000000001");
    nt!(r, s, [cell(s, "A31"), empty()], "0.000123456789012345");
    nt!(r, s, [cell(s, "A32"), empty()], "1.23456789012345E-05");
    nt!(r, s, [cell(s, "A33"), empty()], "1.2345678901234E-06");
    nt!(r, s, [num("1E-18"), empty()], "0.000000000000000001");
    nt!(r, s, [num("1.5E-18"), empty()], "1.5E-18");
    nt!(r, s, [num("1E-19"), empty()], "1E-19");

    // Sums whose 16th and 17th digits differ from their 15-digit text, each computed through
    // calc_func's own + / or - entry before & reads it (the comparison records the & call).
    nt!(
        r,
        s,
        [computed(PLUS, "0.1+0.2", 0.1, 0.2, 0.1 + 0.2), empty()],
        "0.3",
        COMPUTED_OPERAND
    );
    nt!(r, s, [cell(s, "A38"), empty()], "0.3");
    nt!(
        r,
        s,
        [computed(DIVIDE, "1/3", 1.0, 3.0, 1.0 / 3.0), empty()],
        "0.333333333333333",
        COMPUTED_OPERAND
    );
    nt!(
        r,
        s,
        [computed(DIVIDE, "2/3", 2.0, 3.0, 2.0 / 3.0), empty()],
        "0.666666666666667",
        COMPUTED_OPERAND
    );
    nt!(
        r,
        s,
        [computed(MINUS, "1-1E-16", 1.0, 1e-16, 1.0 - 1e-16), empty()],
        "1",
        COMPUTED_OPERAND
    );
    nt!(r, s, [cell(s, "A42"), empty()], "1");

    // Negative numbers, and negative zero, whose text is 0.
    nt!(r, s, [num("-1"), empty()], "-1");
    nt!(r, s, [cell(s, "A44"), empty()], "-0.5");
    nt!(r, s, [cell(s, "A45"), empty()], "-1000000000000000");
    nt!(r, s, [cell(s, "A46"), empty()], "-1.23456789012345E-05");
    nt!(r, s, [cell(s, "A47"), empty()], "-1.7976931348623E+308");
    nt!(r, s, [num("-0"), empty()], "0", STORED_AS_ZERO);
    nt!(r, s, [cell(s, "A49"), empty()], "0", READS_BACK_AS_ZERO);

    // The largest and smallest normal binary64, and the largest and smallest subnormal (of both
    // signs), which Excel holds as 0.
    nt!(r, s, [cell(s, "A50"), empty()], "1.7976931348623E+308");
    nt!(r, s, [cell(s, "A51"), empty()], "2.2250738585072E-308");
    nt!(r, s, [cell(s, "A52"), empty()], "0", READS_BACK_AS_ZERO);
    nt!(r, s, [cell(s, "A53"), empty()], "0", READS_BACK_AS_ZERO);
    nt!(r, s, [cell(s, "A54"), empty()], "0", READS_BACK_AS_ZERO);

    // Three-digit exponents leave 14 digits, and so does an exponent of 99 or -99; the 15 digits
    // are rounded again, a tie away from zero (1.80557226541995E-306 gives 1.80557226542E-306).
    // An exact tie at the 16th digit (7200547821627.375) is refused.
    nt!(r, s, [cell(s, "A55"), empty()], "1.23456789012345E+98");
    nt!(r, s, [cell(s, "A56"), empty()], "1.2345678901235E+99");
    nt!(r, s, [cell(s, "A57"), empty()], "1.2345678901235E+100");
    nt!(r, s, [cell(s, "A58"), empty()], "1.2345678901235E-99");
    nt!(r, s, [num("1E+100"), empty()], "1E+100");
    nt!(r, s, [cell(s, "A60"), empty()], "1.80557226542E-306");
    nt!(
        r,
        s,
        [cell(s, "A61"), empty()],
        "7200547821627.37",
        TIE_TOWARD_ZERO
    );

    // A number on either side, two numbers, and the "op"&cell form of a criterion.
    nt!(r, s, [txt("a"), num("1")], "a1");
    nt!(r, s, [num("1"), txt("a")], "1a");
    nt!(r, s, [txt("="), cell(s, "A2")], "=123456789012345");
    nt!(r, s, [cell(s, "A1"), txt("-")], "1-");
    nt!(
        r,
        s,
        [cell(s, "A2"), cell(s, "A3")],
        "123456789012345999999999999999"
    );

    // The sign does not count toward the 20 characters: a negative number whose magnitude's text
    // takes 20 has no exponent. A negative exact tie at the 16th digit is refused as a positive
    // one is, and a negative tie at the second rounding goes away from zero as a positive one
    // does; a second rounding that carries moves the exponent (9.99999999999995E-99, 14 digits
    // kept).
    nt!(r, s, [cell(s, "A67"), empty()], "-10000000000000000000");
    nt!(r, s, [cell(s, "A68"), empty()], "-99999999999999900000");
    nt!(r, s, [cell(s, "A69"), empty()], "-1E+20");
    nt!(r, s, [cell(s, "A70"), empty()], "-0.000123456789012345");
    nt!(r, s, [num("-1E-18"), empty()], "-0.000000000000000001");
    nt!(
        r,
        s,
        [cell(s, "A72"), empty()],
        "-1234567890123450",
        TIE_TOWARD_ZERO
    );
    nt!(
        r,
        s,
        [cell(s, "A73"), empty()],
        "-7200547821627.37",
        TIE_TOWARD_ZERO
    );
    nt!(r, s, [cell(s, "A74"), empty()], "-1.80557226542E-306");
    nt!(r, s, [cell(s, "A75"), empty()], "1E-98");
    nt!(r, s, [cell(s, "A76"), empty()], "-1E-98");

    // Exact ties at the 16th digit that Excel rounds away from zero, beside the rows above that
    // it rounds toward zero: refused.
    departs!(
        r,
        s,
        [cell(s, "A77"), empty()],
        "521310271410497",
        "521310271410498",
        TIE_AWAY
    );
    departs!(
        r,
        s,
        [cell(s, "A78"), empty()],
        "5758361573115.37",
        "5758361573115.38",
        TIE_AWAY
    );
    departs!(
        r,
        s,
        [cell(s, "A79"), empty()],
        "-6278515587045.87",
        "-6278515587045.88",
        TIE_AWAY
    );
    departs!(
        r,
        s,
        [cell(s, "A80"), empty()],
        "543212677140.062",
        "543212677140.063",
        TIE_AWAY
    );

    // Numbers within a relative 2^-59 of a 16th-digit tie: Excel rounds the first two against
    // their exact value and the third with it; all three are refused. The fourth is that near a
    // tie too, but its 14-digit text is the same on either side. The last two lie just beyond a
    // relative 2^-59 and give their text.
    nt!(
        r,
        s,
        [cell(s, "A81"), empty()],
        "4037429.93595202",
        "5.96e-7 of a unit of the 15th digit above the tie (a relative 2^-69.2); the exact value rounds to 4037429.93595203"
    );
    departs!(
        r,
        s,
        [cell(s, "A82"), empty()],
        "5.24851955262676E-06",
        "5.24851955262677E-06",
        "4.76e-7 of a unit of the 15th digit below the tie (a relative 2^-69.9); the exact value rounds to 5.24851955262676E-06"
    );
    nt!(
        r,
        s,
        [cell(s, "A83"), empty()],
        "-0.919889481585974",
        "2.64e-7 of a unit of the 15th digit below the tie (a relative 2^-71.6); the exact value rounds to the text Excel gives"
    );
    nt!(
        r,
        s,
        [cell(s, "A84"), empty()],
        "-7.3024388533289E+163",
        "1.99e-7 of a unit of the 15th digit above the tie (a relative 2^-71.6), whose 14-digit text is the same on either side"
    );
    nt!(
        r,
        s,
        [cell(s, "A85"), empty()],
        "23.6146129782984",
        "4.34e-4 of a unit of the 15th digit above the tie (a relative 2^-58.9)"
    );
    nt!(
        r,
        s,
        [cell(s, "A86"), empty()],
        "80216.8973582634",
        "1.40e-3 of a unit of the 15th digit below the tie (a relative 2^-58.99)"
    );
    rows
}

#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test)]
fn dc_q9rb__number_text_for_concatenation() {
    const CASE: &str = "dc_q9rb__number_text_for_concatenation";
    let mut sheet = number_text_sheet();
    // The cells hold what the rows name them for.
    for (address, bits) in [
        ("A47", (-f64::MAX).to_bits()),
        ("A49", (-0.0f64).to_bits()),
        ("A50", f64::MAX.to_bits()),
        ("A51", f64::MIN_POSITIVE.to_bits()),
        ("A52", 0x000f_ffff_ffff_ffff),
        ("A53", 0x0000_0000_0000_0001),
        ("A54", 0x8000_0000_0000_0001),
    ] {
        assert_eq!(
            sheet.value(address).as_number().map(f64::to_bits),
            Some(bits),
            "{address}"
        );
    }
    note!("ACCEPT-NOTE: {CASE}: the sheet holds {}", sheet.described());
    let TextRows { rows, .. } = number_text_rows(&mut sheet, false);
    assert!(
        rows.oracle
            .iter()
            .all(|row| row.owner.is_some() == row.outcome.is_err()),
        "a refused row names its owner, a computed row none"
    );
    // cfunc_m1_2__mixed_operands cites these two rows for its rows of the same formulas.
    assert_eq!(
        (
            rows.oracle[61].formula.as_str(),
            rows.oracle[62].formula.as_str()
        ),
        ("=\"a\"&1", "=1&\"a\""),
        "the rows D62 and D63"
    );

    // A number that is no worksheet value is refused, naming its operand.
    let ampersand = surface(AMPERSAND.0, AMPERSAND.1);
    for (operands, argument) in [
        (vec![CalcValue::number(f64::NAN), text_value("")], 0),
        (vec![text_value(""), CalcValue::number(f64::INFINITY)], 1),
        (
            vec![CalcValue::number(f64::NEG_INFINITY), text_value("")],
            0,
        ),
    ] {
        let mut given = Recorded::new(operands);
        assert_eq!(
            run(ampersand, &mut given, &NumericMode::Fast),
            Err(FunctionFault::NotImplemented {
                id: ampersand.id(),
                argument: Some(argument)
            })
        );
    }
    note!(
        "ACCEPT-NOTE: {CASE}: & on NaN, +inf or -inf (no worksheet value) is refused naming that operand"
    );

    // The oracle workbook: its sheet holds exactly the rows' formulas and cells, and the committed
    // workbook is its generator's output.
    assert_workbook_sheet(
        NUMBER_TEXT_WORKBOOK,
        NUMBER_TEXT_WORKBOOK_PATH,
        &rows,
        &sheet,
    );
    #[cfg(all(windows, not(target_arch = "wasm32")))]
    {
        let generated = generated_fixtures();
        let names: Vec<&str> = generated.keys().map(String::as_str).collect();
        assert_eq!(
            names, GENERATED_WORKBOOKS,
            "the generator writes the oracle workbooks and nothing else"
        );
        let regenerated = &generated["dc_q9rb/oracle.xlsx"];
        assert!(
            regenerated.as_slice() == NUMBER_TEXT_WORKBOOK,
            "the committed dc-q9rb oracle workbook is not the generator's output (sha256 {} committed, {} generated)",
            sha256_hex(NUMBER_TEXT_WORKBOOK),
            sha256_hex(regenerated)
        );
        note!(
            "ACCEPT-NOTE: {CASE}: crates/calc-func/tests/fixtures/make-fixtures.ps1, run now under Windows PowerShell, wrote the committed dc-q9rb oracle workbook byte for byte ({} bytes)",
            regenerated.len()
        );
    }
    #[cfg(not(all(windows, not(target_arch = "wasm32"))))]
    {
        note!(
            "ACCEPT-NOTE: {CASE}: the generator runs under Windows PowerShell, so the check that the committed oracle workbook is its output runs on the windows route; here the workbook is checked against the rows only"
        );
    }
    note!(
        "ACCEPT-NOTE: {CASE}: {} rows through calc_func::invoke of &: {} give the text expected for them, the text Excel gives, and {} are refused as not modelled ({}), each beside the text Excel gives; every text is {CAPTURED}",
        rows.oracle.len(),
        rows.oracle.len() - rows.refused(),
        rows.refused(),
        NEAR_TIE.feature()
    );
}

#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test)]
fn dc_q9rb__excel_comparison() {
    const CASE: &str = "dc_q9rb__excel_comparison";
    let TextRows {
        rows,
        excel,
        departs,
    } = number_text_rows(&mut number_text_sheet(), true);
    let (results, evaluations) =
        published_in(NUMBER_TEXT_WORKBOOK, NUMBER_TEXT_CAPTURE_CASE, &[&rows]);
    assert_eq!(
        (results.results.len(), evaluations.evaluations.len()),
        (rows.oracle.len(), rows.oracle.len()),
        "a result and an evaluation for each row"
    );
    let not_modelled = results
        .results
        .iter()
        .filter(|result| matches!(result.provenance, Provenance::NotModeled { .. }))
        .count();
    assert_eq!(
        not_modelled,
        rows.refused(),
        "every refused row is published as not modelled, and only those"
    );
    let capture =
        CaptureRecord::decode(NUMBER_TEXT_CAPTURE).expect("the committed capture decodes");
    assert_eq!(capture.case, NUMBER_TEXT_CAPTURE_CASE);
    assert_eq!(capture.acquisition, Acquisition::LiveCapture);
    let session = capture
        .session
        .as_ref()
        .expect("the committed capture is a version 2 record");
    let workbook = format!("sha256:{}", sha256_hex(NUMBER_TEXT_WORKBOOK));
    if capture.source != workbook {
        note!(
            "ACCEPT-NOT-RUN: {CASE}: awaiting capture: the committed capture is of {}, and the oracle workbook as committed is {workbook} (cargo xtask capture --pending takes it)",
            capture.source
        );
        return;
    }
    let isolation: Vec<_> = session.failures_at(STAGE_ISOLATION).collect();
    assert!(
        isolation.is_empty(),
        "the committed capture records isolation failures: {isolation:?}"
    );
    let environment = capture
        .environment
        .as_ref()
        .expect("the capture records Excel's environment");
    let mut comparison = judged(&capture, &results, &evaluations, &rows)
        .unwrap_or_else(|refusal| panic!("calc_verdict::judge refuses the capture: {refusal}"));
    // The rows whose text departs from Excel's (dc-uar) are mismatched, each with the text the row
    // states; the others are judged as every row is.
    let beside: Vec<(u32, u32)> = departs.iter().map(|&row| (row, FORMULA_COLUMN)).collect();
    comparison.problems = verdict_problems_beside(&comparison.report, &rows, &beside);
    for cell in &comparison.report.cells {
        let address = &cell.formula.cell;
        if address.sheet() == rows.sheet && beside.contains(&(address.row(), address.column())) {
            assert!(
                matches!(cell.verdict, Verdict::Mismatched { .. }),
                "{}: a departing row is {}",
                address.a1(),
                cell.verdict.name()
            );
        }
    }
    assert!(
        comparison.problems.is_empty(),
        "{} problems on the verdict on Excel {}'s capture ({}):\n{}",
        comparison.problems.len(),
        environment.target.excel_build(),
        comparison.report.summary(),
        comparison.problems.join("\n")
    );

    // The text Excel gives, as each row states it (the expected text of a computed row, the text
    // beside a refused one), is the observation the report carries for the row's cell.
    let observed: BTreeMap<u32, &Observation> = comparison
        .report
        .cells
        .iter()
        .filter(|cell| {
            cell.formula.cell.sheet() == rows.sheet && cell.formula.cell.column() == FORMULA_COLUMN
        })
        .filter_map(|cell| Some((cell.formula.cell.row(), cell.observation.as_ref()?)))
        .collect();
    assert_eq!(observed.len(), rows.oracle.len(), "an observation per row");
    for (row, text) in rows.oracle.iter().zip(&excel) {
        let at = format!("{}!{}", rows.sheet, encode_a1(row.row, FORMULA_COLUMN));
        match observed.get(&row.row) {
            Some(Observation::ExcelRaw(value)) => assert!(
                identical(value, &text_value(text)),
                "{at} {}: the row says Excel gives {text:?}, the capture holds {}",
                row.formula,
                shown_value(value)
            ),
            other => panic!("{at}: the report carries no raw value of Excel's: {other:?}"),
        }
    }
    let refused_texts: Vec<String> = rows
        .oracle
        .iter()
        .zip(&excel)
        .filter(|(row, _)| row.owner.is_some())
        .map(|(row, text)| format!("{} {text:?}", encode_a1(row.row, FORMULA_COLUMN)))
        .collect();
    note!(
        "ACCEPT-NOTE: {CASE}: every row's text is Excel's raw value for its cell on the report, the {} refused rows' included: {}",
        refused_texts.len(),
        refused_texts.join(", ")
    );

    // What the label of the rows' texts and the notes of the -0 and subnormal rows state of the
    // capture.
    let setting = |name: &str| {
        environment
            .settings
            .iter()
            .find(|(setting, _)| setting == name)
            .map(|(_, value)| value.as_str())
    };
    for (name, value) in CAPTURED_SETTINGS {
        assert_eq!(setting(name), Some(value), "the capture's setting {name}");
    }
    // The build, the COM add-ins and the package Excel saved (which holds the cells the subnormal
    // and -0 rows read as 0, read when the capture was taken and not retained) are the record's
    // provenance, reported as it states them.
    note!(
        "ACCEPT-NOTE: {CASE}: the capture records Excel {}, COM add-ins connected {:?} ({:?}), and the saved package {:?}",
        environment.target.excel_build(),
        comparison.report.com_add_ins_connected,
        setting(calc_verdict::SETTING_COM_ADD_INS_CONNECTED),
        session.output_package
    );
    let negative_zero = rows
        .oracle
        .iter()
        .find(|row| row.formula == "=-0&\"\"")
        .expect("the row of the literal -0");
    let read_back = capture
        .formulas
        .iter()
        .find(|formula| {
            formula.cell.sheet() == rows.sheet
                && (formula.cell.row(), formula.cell.column())
                    == (negative_zero.row, FORMULA_COLUMN)
        })
        .and_then(|formula| formula.formula.as_deref());
    assert_eq!(read_back, Some("=0&\"\""), "Excel's formula of the -0 row");
    note!(
        "ACCEPT-NOTE: {CASE}: the capture was taken by Excel {} under {}; the formula of row {} (=-0&\"\") reads back from Excel as =0&\"\"",
        environment.target.excel_build(),
        CAPTURED_SETTINGS
            .iter()
            .map(|(name, value)| format!("{name} {value:?}"))
            .collect::<Vec<_>>()
            .join(", "),
        encode_a1(negative_zero.row, FORMULA_COLUMN)
    );
    note!(
        "ACCEPT-NOTE: {CASE}: Excel comparison through calc_verdict::judge against Excel {}'s capture of the oracle workbook: {} (every computed row matched, every refused row not modeled with its owner); the whole workbook: {}",
        environment.target.excel_build(),
        sheet_verdicts(&comparison.report, &rows),
        comparison.report.summary()
    );
}

// ---------------------------------------------------------------------------------------------
// cfunc-m3.6: the lookup and reference functions (INDEX, MATCH, XMATCH, XLOOKUP, VLOOKUP,
// HLOOKUP, LOOKUP, CHOOSE, ROW, COLUMN, ROWS, COLUMNS and AREAS) through calc_func::invoke on
// catalogue targets.
//
// Each oracle row runs one formula's calls through calc_func::invoke, with the test sheet as the
// cell reader, and asserts a result. A measured row asserts the value Excel 16.0 build 20430
// shows in the cell for the same formula over the same cells, which the committed capture of the
// oracle workbook holds and cfunc_m3_6__excel_comparison judges. A row whose result is a reference
// to several cells, an array or a blank result asserts calc-func's own result, a documented rule
// or an unmeasured choice that its label names, and states what Excel shows in that cell instead
// (the cell publication of such a result is another bead's); the comparison checks that statement
// against the capture. A row labelled `refused` is a call calc-func refuses, with the bead that
// owns Excel's behaviour, and is counted as unsupported in the per-ID manifest of
// `migration/ports/cfunc-m3.6.json`; where it states what Excel shows, the comparison checks that
// too.
//
// A row's result is what calc_func returns: a reference stays a reference. Two kinds of row
// stand in for calc-fml here: a row whose result is a reference to one cell asserts the value the
// cell shows as the test's own dereference, and a nested formula (SUM(INDEX(...)),
// INDEX(B:B,MATCH(...))) runs its inner call first and gives its result to the outer call. Both
// are published to the comparison as not modeled, owned by dc-l10u, which judges them through
// calc_fml::evaluate against the same capture.
//
// `tests/fixtures/cfunc_m3_6/oracle.xlsx`, written by the committed generator
// `tests/fixtures/make-fixtures.ps1`, holds every row's formula in column D of a sheet named after
// its case, beside the cells the rows read (XLOOKUP and XMATCH spelled `_xlfn.` as a file stores
// them). The comparison case judges the rows of both sheets against the committed capture of that
// workbook through calc_verdict::judge (`LOOKUP_CAPTURE_COMMAND` writes it).

/// The lookup oracle workbook, as committed (its bytes are kept across checkouts by
/// `.gitattributes`).
const LOOKUP_WORKBOOK: &[u8] = include_bytes!("fixtures/cfunc_m3_6/oracle.xlsx");
const LOOKUP_WORKBOOK_PATH: &str = "crates/calc-func/tests/fixtures/cfunc_m3_6/oracle.xlsx";
/// Excel's capture of the lookup oracle workbook (a version-2 live capture; an absent-capture
/// marker here would make the comparison case print ACCEPT-NOT-RUN with MissingCapture).
const LOOKUP_CAPTURE: &[u8] = include_bytes!("captures/cfunc_m3_6/oracle.capture.json");
/// The case the capture is recorded under.
const LOOKUP_CAPTURE_CASE: &str = "cfunc_m3_6__excel_comparison";
/// The command that writes the capture.
const LOOKUP_CAPTURE_COMMAND: &str = "cargo xtask capture --fixture crates/calc-func/tests/fixtures/cfunc_m3_6/oracle.xlsx --case cfunc_m3_6__excel_comparison --out crates/calc-func/tests/captures/cfunc_m3_6/oracle.capture.json";
/// The bead's migration record, whose per-ID manifest the edge case checks against the catalogue.
const LOOKUP_RECORD: &str = include_str!("../../../migration/ports/cfunc-m3.6.json");

/// The family's entries: each spelling and the member number of its FunctionId in family 9.
const LOOKUPS: [(&str, u32); 13] = [
    ("INDEX", 1),
    ("MATCH", 2),
    ("XMATCH", 3),
    ("XLOOKUP", 4),
    ("VLOOKUP", 5),
    ("HLOOKUP", 6),
    ("LOOKUP", 7),
    ("CHOOSE", 8),
    ("ROW", 9),
    ("COLUMN", 10),
    ("ROWS", 11),
    ("COLUMNS", 12),
    ("AREAS", 13),
];

/// Where a measured row's expectation comes from.
const MEASURED: &str = "measured: Excel 16.0 build 20430 shows this value in the cell (the committed capture of the oracle workbook, which cfunc_m3_6__excel_comparison judges)";

/// What a lookup row's call gives.
#[derive(Clone, Debug)]
enum Gives {
    /// A value or a refusal, as [`Expect`] states it.
    Is(Expect),
    /// A reference to this one cell of the test sheet, which shows the expected value (a blank
    /// cell 0).
    Cell(&'static str, Expect),
    /// A reference to this area of several cells of the test sheet.
    Area(&'static str),
    /// Empty: a blank cell's value, or an omitted value chosen.
    Blank,
    /// This array.
    Array(Vec<Vec<f64>>),
}

/// Why a lookup row expects its result.
#[derive(Clone, Debug)]
enum Seen {
    /// Excel showed this result in the cell for the same formula over the same cells
    /// ([`MEASURED`]).
    Measured,
    /// calc-func's result is a reference to several cells, an array or blank, which Excel shows
    /// in the cell as `shows`; `basis` says why calc-func gives the result it gives.
    Shape { basis: &'static str, shows: Expect },
    /// calc-func refuses the call; Excel's behaviour belongs to this owner, and, when stated, Excel
    /// shows this value in the cell, which the comparison checks against the capture.
    Refused(Owner, Option<Expect>),
}

const SPILL_OWNER: Owner = Owner {
    bead: "dc-10y",
    behaviour: "an array result published in a cell",
};
const AREA_OWNER: Owner = Owner {
    bead: "dc-10y",
    behaviour: "a reference to several cells as a cell's value (implicit intersection)",
};
const BLANK_OWNER: Owner = Owner {
    bead: "dc-10y",
    behaviour: "a blank function result published in a cell as 0",
};
const LIFT_OWNER: Owner = Owner {
    bead: "dc-10y",
    behaviour: "a function lifted over an array argument",
};
/// The rows that stand in for calc-fml: a reference result dereferenced at the formula's root, or
/// a nested call's result given to the outer call.
const ENGINE_ROUTE: Owner = Owner {
    bead: "dc-l10u",
    behaviour: "a reference result or a nested call evaluated through calc_fml::evaluate",
};
/// The calling cell, which ROW() and COLUMN() without an argument need.
const CALLER_OWNER: Owner = Owner {
    bead: "dc-10y",
    behaviour: "the calling cell supplied to a kernel",
};
/// A read past the reference a formula gives, which the engine does not see as a prerequisite.
const READ_PAST_OWNER: Owner = Owner {
    bead: "dc-10y",
    behaviour: "cells read past the reference given, as prerequisites of the formula",
};

/// Why an INDEX of a reference with a 0 index gives a reference to several cells.
const INDEX_AREA: &str = "a reference to the cells selected, documented (Microsoft's INDEX page: row_num or column_num 0 returns the reference of the whole column or row); captured: AREAS(INDEX(A1:C3,0,2)) is 1, and AREAS of a value that is not a reference is #VALUE!";
/// Why an INDEX of an array with a 0 index gives an array.
const INDEX_ARRAY: &str = "the array of the column selected, documented (Microsoft's INDEX page: column_num of an array with row_num 0 returns the whole column); captured: ROWS and SUM of it are 2 and 6";
/// Why a lookup of a blank cell gives Empty.
const BLANK_RESULT: &str = "Empty, an unmeasured choice of value consistent with the capture: & joins VLOOKUP of a blank cell and CHOOSE of an omitted value as empty text, where a 0 would join as \"0\"";
/// Why XLOOKUP with a lookup array of one cell gives the whole return array of one row or column.
const XLOOKUP_ONE_CELL: &str = "the whole return array of one row or column; captured: ROWS, COLUMNS and SUM of XLOOKUP(1,A1,A13:C13) are 1, 3 and 6, and of XLOOKUP(1,A1,C1:C3) 3, 1 and 60";
/// Why XLOOKUP gives the row or column of a return array of several rows and columns.
const XLOOKUP_PART: &str = "the row (a vertical lookup array) or column (a horizontal one) of the return array at the match, documented (Microsoft's XLOOKUP page: a return array of several columns or rows returns them all), a reference into a return reference";
/// ROW and COLUMN of several rows or columns: calc-func gives the array of every number, which a
/// dynamic-array formula spills, and a legacy formula (an xlsx cell without dynamic-array
/// metadata, as the oracle workbook stores its formulas) takes the first of through the `@`
/// Excel writes before the call, which CalcFml's compile writes too (dc-b1l).
const LEGACY_POSITIONS: &str = "the array of every row's or column's number, of which a legacy formula takes the first (Excel writes @ before the call)";

/// The errors XMATCH orders, in its order: the eight a cell constant can hold, then #SPILL!,
/// #FIELD! and #CALC!, which only a formula gives (Excel refuses to open a workbook holding them
/// as cell constants and refuses them as formula literals).
const ORDERED_ERRORS: [(WorksheetErrorCode, &str); 11] = [
    (WorksheetErrorCode::Null, "#NULL!"),
    (WorksheetErrorCode::Div0, "#DIV/0!"),
    (WorksheetErrorCode::Value, "#VALUE!"),
    (WorksheetErrorCode::Ref, "#REF!"),
    (WorksheetErrorCode::Name, "#NAME?"),
    (WorksheetErrorCode::Num, "#NUM!"),
    (WorksheetErrorCode::NA, "#N/A"),
    (WorksheetErrorCode::GettingData, "#GETTING_DATA"),
    (WorksheetErrorCode::Spill, "#SPILL!"),
    (WorksheetErrorCode::Field, "#FIELD!"),
    (WorksheetErrorCode::Calc, "#CALC!"),
];

fn error_name(code: WorksheetErrorCode) -> &'static str {
    ORDERED_ERRORS
        .iter()
        .find(|(known, _)| *known == code)
        .map(|(_, name)| *name)
        .unwrap_or_else(|| panic!("{code:?} is not one of the errors XMATCH orders here"))
}

/// The functions of the source formulas, which calc_func's catalogue does not have, and the error
/// Excel gives each source formula of the lookup oracle workbook.
const SOURCE_CALC: Owner = Owner {
    bead: "dc-j9k",
    behaviour: "FILTER, here with no cell of A1:A6 above 100, which Excel gives #CALC!",
};
const SOURCE_SPILL: Owner = Owner {
    bead: "dc-j9k, dc-10y",
    behaviour: "SEQUENCE(1,2), whose spill the filled cell to its right blocks, which Excel gives #SPILL!",
};
const SOURCE_FIELD: Owner = Owner {
    bead: "dc-uar",
    behaviour: "FIELDVALUE of a number, which holds no data type, which Excel gives #FIELD!",
};
/// A row that reads a source cell: its value rests on the error the reader holds for the source.
const SOURCE_READ: Owner = Owner {
    bead: "dc-j9k, dc-10y, dc-uar",
    behaviour: "a value conditional on the errors Excel gives the source formulas it reads (FILTER, SEQUENCE and FIELDVALUE), which the reader holds",
};

/// The source formula of the lookup oracle workbook that gives each error no cell constant holds:
/// as entered, as the file stores it, and the beads that own it.
fn error_source(code: WorksheetErrorCode) -> Option<(&'static str, &'static str, Owner)> {
    match code {
        WorksheetErrorCode::Spill => Some(("SEQUENCE(1,2)", "_xlfn.SEQUENCE(1,2)", SOURCE_SPILL)),
        WorksheetErrorCode::Field => Some((
            "FIELDVALUE(A1,\"x\")",
            "_xlfn.FIELDVALUE(A1,\"x\")",
            SOURCE_FIELD,
        )),
        WorksheetErrorCode::Calc => Some((
            "FILTER(A1:A6,A1:A6>100)",
            "_xlfn._xlws.FILTER(A1:A6,A1:A6>100)",
            SOURCE_CALC,
        )),
        _ => None,
    }
}

/// A source formula of the sheet lookup_edges: a call to a function calc_func's catalogue does not
/// have, which the workbook holds as a dynamic-array formula over its one cell. The test sheet's
/// reader holds for it the error Excel gives it, so a row that reads it is conditional on that
/// error: such a row is published as not modeled, and the comparison checks both the source
/// cell's error and the row's value against the capture's raw values.
struct SourceCell {
    a1: String,
    entered: &'static str,
    stored: &'static str,
    error: WorksheetErrorCode,
    owner: Owner,
}

/// Every ordered pair of two different errors of [`ORDERED_ERRORS`] with #SPILL!, #FIELD! or
/// #CALC! among them, by first error and then second: pair k is P{k}:Q{k} of the sheet
/// lookup_edges.
fn source_error_pairs() -> Vec<(WorksheetErrorCode, WorksheetErrorCode)> {
    let mut pairs = Vec::new();
    for (first, _) in ORDERED_ERRORS {
        for (second, _) in ORDERED_ERRORS {
            if first != second && (error_source(first).is_some() || error_source(second).is_some())
            {
                pairs.push((first, second));
            }
        }
    }
    pairs
}

/// The source cells of the oracle sheet `sheet`: on lookup_edges, each cell of the pairs P:Q that
/// holds #SPILL!, #FIELD! or #CALC!; reference_result has none.
fn lookup_sources(sheet: &str) -> Vec<SourceCell> {
    let mut sources = Vec::new();
    if sheet != "lookup_edges" {
        return sources;
    }
    for (row, (first, second)) in source_error_pairs().into_iter().enumerate() {
        for (column, code) in [("P", first), ("Q", second)] {
            if let Some((entered, stored, owner)) = error_source(code) {
                sources.push(SourceCell {
                    a1: format!("{column}{}", row + 1),
                    entered,
                    stored,
                    error: code,
                    owner,
                });
            }
        }
    }
    sources
}

impl Gives {
    /// Whether `result` is this, for the entry `id`.
    fn holds(
        &self,
        s: &TestSheet,
        id: FunctionId,
        result: &Result<CalcValue, FunctionFault>,
    ) -> bool {
        match (self, result) {
            (Gives::Is(expect), result) => expect.holds(id, result),
            (Gives::Cell(a1, shows), Ok(value)) => {
                value.as_reference() == Some(&ReferenceLike::cell(s.key(a1)))
                    && shows.holds(id, &Ok(shown_at_root(s, value)))
            }
            (Gives::Area(text), Ok(value)) => {
                value.as_reference() == Some(&ReferenceLike::area(s.area(text)))
            }
            (Gives::Blank, Ok(value)) => value.is_empty(),
            (Gives::Array(rows), Ok(value)) => {
                let expected = CalcArray::from_rows(
                    rows.iter()
                        .map(|row| row.iter().map(|n| CalcValue::number(*n)).collect())
                        .collect(),
                )
                .expect("a rectangular array");
                value.as_array() == Some(&expected)
            }
            _ => false,
        }
    }

    fn shown(&self) -> String {
        match self {
            Gives::Is(expect) => expect.shown(),
            Gives::Cell(a1, shows) => format!("a reference to {a1}, which shows {}", shows.shown()),
            Gives::Area(text) => format!("a reference to {text}"),
            Gives::Blank => "Empty".to_owned(),
            Gives::Array(rows) => format!("the array {rows:?}"),
        }
    }

    /// The bead that owns the cell publication of this result, when the comparison does not score
    /// calc_func's result as the cell's value: a reference to one cell is dereferenced by
    /// calc_fml (dc-l10u judges it there), and a reference to several cells, an array and a blank
    /// result are published in a cell by calc_fml's contexts.
    fn publication_owner(&self) -> Option<Owner> {
        match self {
            Gives::Area(_) => Some(AREA_OWNER),
            Gives::Blank => Some(BLANK_OWNER),
            Gives::Array(_) => Some(SPILL_OWNER),
            Gives::Cell(..) => Some(ENGINE_ROUTE),
            Gives::Is(_) => None,
        }
    }

    /// Whether the cell shows something other than calc_func's result itself.
    fn is_shape(&self) -> bool {
        matches!(self, Gives::Area(_) | Gives::Array(_) | Gives::Blank)
    }
}

/// What each row that computes a value states Excel shows in its cell, by sheet and row, and
/// whether the row is one of [`Seen::Shape`].
type Stated = BTreeMap<(&'static str, u32), (Expect, bool)>;

thread_local! {
    /// The formulas of the nested calls made on this thread ([`nested`]): an argument with one of
    /// these texts is a nested call's result.
    static NESTED: std::cell::RefCell<BTreeSet<String>> = const { std::cell::RefCell::new(BTreeSet::new()) };
    /// The rows' statements of what Excel shows ([`Stated`]).
    static SHOWN: std::cell::RefCell<Stated> = const { std::cell::RefCell::new(BTreeMap::new()) };
}

/// The value the test shows for a reference to one cell of the test sheet (a blank cell 0), the
/// test's own stand-in for calc_fml's dereference: rows that use it are published as not modeled
/// under dc-l10u.
fn shown_at_root(s: &TestSheet, value: &CalcValue) -> CalcValue {
    match value.as_reference().map(ReferenceLike::areas) {
        Some([area]) if area.cell_count() == 1 => {
            let key = area.first();
            match s.cells.get(&(key.row(), key.column())) {
                Some(held) => held.clone(),
                None => CalcValue::number(0.0),
            }
        }
        _ => value.clone(),
    }
}

/// A nested call as an argument of the outer call: the inner call runs now through
/// calc_func::invoke over the test sheet, and its result (which must not be a fault) is what the
/// outer call receives, as calc-fml gives an eager call the value of an argument expression.
fn nested(s: &mut TestSheet, (notation, spelling): (Notation, &str), args: &[Arg]) -> Arg {
    let target = surface(notation, spelling);
    let texts: Vec<&str> = args.iter().map(|a| a.excel.as_str()).collect();
    let formula = target
        .oracle_formula(&texts)
        .unwrap_or_else(|| panic!("{spelling} does not admit {texts:?}"));
    let mut recorded = Recorded::new(args.iter().map(|a| a.value.clone()).collect());
    let value = invoke(
        target,
        &mut recorded,
        &mut EvalServices::with_cells(&NumericMode::Fast, s),
        &mut FunctionCallScratch::new(),
    )
    .unwrap_or_else(|fault| panic!("{formula}: the inner call ends in {fault:?}"));
    let excel = formula[1..].to_owned();
    NESTED.with(|nested| nested.borrow_mut().insert(excel.clone()));
    Arg {
        excel,
        value,
        reads: args.iter().flat_map(|a| a.reads.clone()).collect(),
    }
}

/// A nested call's result as a legacy formula's `@` gives it to the enclosing call: an array's
/// first element (CalcFml's implicit intersection of an array).
fn at_first(arg: Arg) -> Arg {
    let value = match arg.value.core() {
        CoreValue::Array(array) => array.get(0, 0).cloned().expect("an array holds an element"),
        _ => arg.value.clone(),
    };
    Arg { value, ..arg }
}

/// Whether `arg` is a nested call's result ([`nested`]).
fn is_nested(arg: &Arg) -> bool {
    NESTED.with(|nested| nested.borrow().contains(&arg.excel))
}

/// A nested call as an argument: `nest!(sheet, "SPELLING", [args])` (see [`nested`]).
macro_rules! nest {
    ($sheet:expr, $spelling:expr, [$($arg:expr),* $(,)?]) => {{
        let args = [$($arg),*];
        nested($sheet, named($spelling), &args)
    }};
}

/// The areas of the reference arguments of a call.
fn reference_areas(args: &[Arg]) -> Vec<GridArea> {
    args.iter()
        .filter_map(|a| a.value.as_reference())
        .flat_map(|reference| reference.areas().to_vec())
        .collect()
}

/// Runs the call `surface` on `args` through calc_func::invoke under Fast with the test sheet as
/// the cell reader and asserts `gives`, the arguments the call interface asked for (each once in
/// order for an eager entry; index_num and then at most the chosen value for CHOOSE), and that
/// every area the reader was asked for lies inside a reference argument. Then records the row,
/// with the next cell of column D, and prints it with its basis.
fn lookup_row(
    rows: &mut Rows,
    s: &mut TestSheet,
    surface_of: (Notation, &str),
    args: &[Arg],
    gives: Gives,
    seen: Seen,
) {
    let target = surface(surface_of.0, surface_of.1);
    let texts: Vec<&str> = args.iter().map(|a| a.excel.as_str()).collect();
    let formula = target
        .oracle_formula(&texts)
        .unwrap_or_else(|| panic!("{surface_of:?} does not admit {texts:?}"));
    let mut recorded = Recorded::new(args.iter().map(|a| a.value.clone()).collect());
    let reads_before = s.reads.len();
    let tick = next_tick();
    let result = invoke(
        target,
        &mut recorded,
        &mut EvalServices::with_cells(&NumericMode::Fast, s),
        &mut FunctionCallScratch::new(),
    );
    assert!(
        gives.holds(s, target.id(), &result),
        "{formula}: expected {}, got {result:?}",
        gives.shown()
    );
    if target.argument_demand().is_lazy() {
        assert_eq!(
            recorded.asked.first(),
            Some(&0),
            "{formula}: asked {:?}",
            recorded.asked
        );
        assert!(
            recorded.asked.len() <= 2,
            "{formula}: asked {:?}",
            recorded.asked
        );
    } else {
        let in_order: Vec<usize> = (0..args.len()).collect();
        assert_eq!(
            recorded.asked, in_order,
            "{formula}: the arguments asked for"
        );
    }
    let given = reference_areas(args);
    for read in &s.reads[reads_before..] {
        assert!(
            given
                .iter()
                .any(|area| { area.contains(read.first()) && area.contains(read.last()) }),
            "{formula}: the reader was asked for {read:?}, outside the references given"
        );
    }
    let through_nesting = args.iter().any(is_nested);
    let sources = lookup_sources(rows.sheet);
    let reads_source = s.reads[reads_before..].iter().any(|area| {
        sources
            .iter()
            .any(|source| area.contains(s.key(&source.a1)))
    });
    let owner = match &seen {
        Seen::Measured | Seen::Shape { .. } => {
            assert!(result.is_ok(), "{formula}: the row computes a value");
            assert_eq!(
                matches!(seen, Seen::Shape { .. }),
                gives.is_shape(),
                "{formula}: a row whose result Excel does not show as it is states what Excel shows"
            );
            if through_nesting {
                Some(ENGINE_ROUTE)
            } else if reads_source {
                Some(SOURCE_READ)
            } else {
                gives.publication_owner()
            }
        }
        Seen::Refused(owner, _) => {
            assert!(
                result.is_err(),
                "{formula}: a refused row computed {result:?}"
            );
            Some(*owner)
        }
    };
    let row = u32::try_from(rows.oracle.len() + 1).expect("a row number");
    let in_cell = match (&seen, &gives) {
        (Seen::Shape { shows, .. }, _) => Some((shows.clone(), true)),
        (Seen::Measured, Gives::Is(expect)) => Some((expect.clone(), false)),
        (Seen::Measured, Gives::Cell(_, shows)) => Some((shows.clone(), false)),
        (Seen::Refused(_, Some(shows)), _) => Some((shows.clone(), false)),
        _ => None,
    };
    SHOWN.with(|shown| match in_cell {
        Some(stated) => shown.borrow_mut().insert((rows.sheet, row), stated),
        None => shown.borrow_mut().remove(&(rows.sheet, row)),
    });
    if !rows.quiet {
        let reads: Vec<&str> = args
            .iter()
            .flat_map(|a| a.reads.iter().map(String::as_str))
            .collect();
        let cells = if reads.is_empty() {
            String::new()
        } else {
            format!(" with {}", reads.join(", "))
        };
        let at = format!("{}!{}", rows.sheet, encode_a1(row, FORMULA_COLUMN));
        let label = match (&seen, &result) {
            (Seen::Measured, _) => MEASURED.to_owned(),
            (Seen::Shape { basis, shows }, _) => format!(
                "calc-func's result is {basis}; Excel shows {} in this cell",
                shows.shown()
            ),
            (Seen::Refused(owner, shows), Err(fault)) => format!(
                "refused with {fault:?}; Excel's behaviour is owned by {}: {}{}",
                owner.bead,
                owner.behaviour,
                shows
                    .as_ref()
                    .map(|shows| format!("; Excel shows {} in this cell", shows.shown()))
                    .unwrap_or_default()
            ),
            (Seen::Refused(..), Ok(_)) => unreachable!("checked above"),
        };
        let published = match owner {
            Some(owner) if result.is_ok() => format!(
                "; published to the comparison as not modeled, owned by {}: {}",
                owner.bead, owner.behaviour
            ),
            _ => String::new(),
        };
        note!(
            "ACCEPT-NOTE: {}: oracle {at} {formula}{cells} -> {} ({label}{published})",
            rows.case,
            gives.shown()
        );
    }
    rows.calls.push(Call { row, tick });
    rows.oracle.push(OracleRow {
        row,
        formula,
        outcome: result,
        choice: None,
        owner,
        tick,
    });
}

/// An array constant as a formula writes it, as the caller supplies it: numbers, "text", TRUE,
/// FALSE and the error literals of these fixtures, `,` between columns and `;` between rows (no
/// text holding either).
fn arr(excel: &str) -> Arg {
    let inner = excel
        .strip_prefix('{')
        .and_then(|rest| rest.strip_suffix('}'))
        .unwrap_or_else(|| panic!("{excel} is no array constant"));
    let element = |item: &str| -> CalcValue {
        if let Some(text) = item
            .strip_prefix('"')
            .and_then(|rest| rest.strip_suffix('"'))
        {
            return text_value(&text.replace("\"\"", "\""));
        }
        match item {
            "TRUE" => CalcValue::logical(true),
            "FALSE" => CalcValue::logical(false),
            _ => match ERROR_LITERALS.iter().find(|(_, literal)| *literal == item) {
                Some((code, _)) => CalcValue::error(*code),
                None => CalcValue::number(
                    item.parse()
                        .unwrap_or_else(|_| panic!("{item} is no array element")),
                ),
            },
        }
    };
    array_arg(
        excel,
        inner
            .split(';')
            .map(|row| row.split(',').map(element).collect())
            .collect(),
    )
}

/// Runs a measured row: `lk!(rows, sheet, "SPELLING", [args], gives)`.
macro_rules! lk {
    ($rows:expr, $sheet:expr, $spelling:expr, [$($arg:expr),* $(,)?], $gives:expr) => {{
        let args = [$($arg),*];
        lookup_row($rows, $sheet, named($spelling), &args, $gives, Seen::Measured)
    }};
}

/// Runs a row whose result Excel does not show as it is:
/// `lk_shape!(rows, sheet, "SPELLING", [args], gives, basis, shows)`.
macro_rules! lk_shape {
    ($rows:expr, $sheet:expr, $spelling:expr, [$($arg:expr),* $(,)?], $gives:expr, $basis:expr, $shows:expr) => {{
        let args = [$($arg),*];
        lookup_row(
            $rows,
            $sheet,
            named($spelling),
            &args,
            $gives,
            Seen::Shape {
                basis: $basis,
                shows: $shows,
            },
        )
    }};
}

/// Runs a refused row: `lk_refused!(rows, sheet, owner, "SPELLING", [args], gives)`.
macro_rules! lk_refused {
    ($rows:expr, $sheet:expr, $owner:expr, $spelling:expr, [$($arg:expr),* $(,)?], $gives:expr) => {{
        let args = [$($arg),*];
        lookup_row($rows, $sheet, named($spelling), &args, $gives, Seen::Refused($owner, None))
    }};
}

/// Runs a refused row that states what Excel shows in its cell:
/// `lk_refused_shows!(rows, sheet, owner, "SPELLING", [args], gives, shows)`.
macro_rules! lk_refused_shows {
    ($rows:expr, $sheet:expr, $owner:expr, $spelling:expr, [$($arg:expr),* $(,)?], $gives:expr, $shows:expr) => {{
        let args = [$($arg),*];
        lookup_row(
            $rows,
            $sheet,
            named($spelling),
            &args,
            $gives,
            Seen::Refused($owner, Some($shows)),
        )
    }};
}

/// An expected number.
fn n(value: f64) -> Gives {
    Gives::Is(N(value))
}

/// An expected text.
fn tx(text: &str) -> Gives {
    Gives::Is(t(text))
}

/// An expected error.
fn er(code: WorksheetErrorCode) -> Gives {
    Gives::Is(E(code))
}

/// An expected refusal naming argument `argument`.
fn refused(argument: Option<usize>) -> Gives {
    Gives::Is(Refused(argument))
}

/// A reference to one cell that shows a number.
fn at_n(a1: &'static str, value: f64) -> Gives {
    Gives::Cell(a1, N(value))
}

/// Prints how many rows a case ran and on what basis.
fn lookup_rows_note(rows: &Rows) {
    let refused = rows.refused();
    let shapes = SHOWN.with(|shown| {
        shown
            .borrow()
            .iter()
            .filter(|((sheet, _), (_, shape))| *sheet == rows.sheet && *shape)
            .count()
    });
    let engine = rows
        .oracle
        .iter()
        .filter(|row| row.owner == Some(ENGINE_ROUTE))
        .count();
    let conditional = rows
        .oracle
        .iter()
        .filter(|row| row.owner == Some(SOURCE_READ))
        .count();
    let elsewhere = rows
        .oracle
        .iter()
        .filter(|row| row.outcome.is_ok() && row.owner.is_some())
        .count()
        - engine
        - conditional;
    note!(
        "ACCEPT-NOTE: {}: {} rows through calc_func::invoke, each with its formula on sheet {} of {LOOKUP_WORKBOOK_PATH}: {} asserting the value Excel shows in the cell, measured; {} asserting calc-func's result where Excel shows another (a reference to several cells, an array or a blank result), each stating what Excel shows; {} refused with the bead that owns Excel's behaviour. Published to the comparison as not modeled: {} rows that stand in for calc_fml (a reference to one cell dereferenced by the test, or a nested call), owned by dc-l10u; {} rows conditional on the errors the reader holds for the source formulas they read (owned by {}: {}); and {} other computed rows whose cell publication another bead owns",
        rows.case,
        rows.oracle.len(),
        rows.sheet,
        rows.oracle.len() - refused - shapes,
        shapes,
        refused,
        engine,
        conditional,
        SOURCE_READ.bead,
        SOURCE_READ.behaviour,
        elsewhere
    );
}

/// `rows` with each formula as the workbook stores it: XLOOKUP and XMATCH carry the `_xlfn.`
/// prefix of functions newer than the file format.
fn stored_formulas(rows: &Rows) -> Rows {
    let mut stored = Rows::new(rows.case, rows.sheet, rows.quiet);
    for row in &rows.oracle {
        stored.oracle.push(OracleRow {
            row: row.row,
            formula: row
                .formula
                .replace("XLOOKUP(", "_xlfn.XLOOKUP(")
                .replace("XMATCH(", "_xlfn.XMATCH("),
            outcome: row.outcome.clone(),
            choice: row.choice,
            owner: row.owner,
            tick: row.tick,
        });
    }
    stored
}

/// Checks the lookup oracle workbook's sheet for `rows` and, on the route where the generator
/// runs, that the committed workbook is its output.
fn assert_lookup_workbook(rows: &Rows, s: &TestSheet) {
    let sources: BTreeMap<(u32, u32), String> = lookup_sources(rows.sheet)
        .into_iter()
        .map(|source| (decode_a1(&source.a1), source.stored.to_owned()))
        .collect();
    assert_workbook_sheet_with_sources(
        LOOKUP_WORKBOOK,
        LOOKUP_WORKBOOK_PATH,
        &stored_formulas(rows),
        s,
        &sources,
    );
    let case = rows.case;
    #[cfg(all(windows, not(target_arch = "wasm32")))]
    {
        let generated = generated_fixtures();
        let names: Vec<&str> = generated.keys().map(String::as_str).collect();
        assert_eq!(
            names, GENERATED_WORKBOOKS,
            "the generator writes the oracle workbooks and nothing else"
        );
        let regenerated = &generated["cfunc_m3_6/oracle.xlsx"];
        assert!(
            regenerated.as_slice() == LOOKUP_WORKBOOK,
            "the committed lookup oracle workbook is not the generator's output (sha256 {} committed, {} generated)",
            sha256_hex(LOOKUP_WORKBOOK),
            sha256_hex(regenerated)
        );
        note!(
            "ACCEPT-NOTE: {case}: crates/calc-func/tests/fixtures/make-fixtures.ps1, run now under Windows PowerShell, wrote the committed lookup oracle workbook byte for byte ({} bytes)",
            regenerated.len()
        );
    }
    #[cfg(not(all(windows, not(target_arch = "wasm32"))))]
    {
        note!(
            "ACCEPT-NOTE: {case}: the generator runs under Windows PowerShell, so the check that the committed lookup oracle workbook is its output runs on the windows route; here the workbook is checked against the rows only"
        );
    }
}

// ---------------------------------------------------------------------------------------------
// cfunc_m3_6__reference_result

/// The cells the reference rows read: A1:C3 hold 1 to 9, A4 text, B4 is blank, C4 TRUE; A10:A14
/// hold keys k1 to k5, B10:B14 numbers with B13 blank, C10:C14 text; far down column A, A1000
/// holds the key kz and C1000 the number 77. Every other cell is blank.
fn reference_result_sheet() -> TestSheet {
    let num = |v: f64| CalcValue::number(v);
    TestSheet::new(&[
        ("A1", "1", num(1.0)),
        ("B1", "2", num(2.0)),
        ("C1", "3", num(3.0)),
        ("A2", "4", num(4.0)),
        ("B2", "5", num(5.0)),
        ("C2", "6", num(6.0)),
        ("A3", "7", num(7.0)),
        ("B3", "8", num(8.0)),
        ("C3", "9", num(9.0)),
        ("A4", "text \"t\"", text_value("t")),
        ("C4", "TRUE", CalcValue::logical(true)),
        ("A10", "text \"k1\"", text_value("k1")),
        ("A11", "text \"k2\"", text_value("k2")),
        ("A12", "text \"k3\"", text_value("k3")),
        ("A13", "text \"k4\"", text_value("k4")),
        ("A14", "text \"k5\"", text_value("k5")),
        ("B10", "10", num(10.0)),
        ("B11", "20", num(20.0)),
        ("B12", "30", num(30.0)),
        ("B14", "50", num(50.0)),
        ("C10", "text \"x10\"", text_value("x10")),
        ("C11", "text \"x20\"", text_value("x20")),
        ("C12", "text \"x30\"", text_value("x30")),
        ("C13", "text \"x40\"", text_value("x40")),
        ("C14", "text \"x50\"", text_value("x50")),
        ("A1000", "text \"kz\"", text_value("kz")),
        ("C1000", "77", num(77.0)),
    ])
}

/// The rows of cfunc_m3_6__reference_result, in the order the lookup oracle workbook's sheet
/// `reference_result` holds their formulas.
fn reference_result_rows(s: &mut TestSheet, quiet: bool) -> Rows {
    const CASE: &str = "cfunc_m3_6__reference_result";
    let mut rows = Rows::new(CASE, "reference_result", quiet);
    let r = &mut rows;
    use WorksheetErrorCode::{NA, Ref, Value};
    let grid = |s: &TestSheet| reference(s, &["A1:C3"]);
    let two_areas = |s: &TestSheet| reference(s, &["A1:A3", "C1:C3"]);

    // INDEX of a reference gives a reference: to a cell, which shows its value (a blank cell 0),
    // or to a row, a column or the whole area.
    lk!(
        r,
        s,
        "INDEX",
        [grid(s), num("2"), num("3")],
        at_n("C2", 6.0)
    );
    lk!(
        r,
        s,
        "INDEX",
        [grid(s), num("2.7"), num("1.2")],
        at_n("A2", 4.0)
    );
    lk!(
        r,
        s,
        "INDEX",
        [reference(s, &["A1:A4"]), num("4")],
        Gives::Cell("A4", t("t"))
    );
    lk!(
        r,
        s,
        "INDEX",
        [reference(s, &["A1:C1"]), num("3")],
        at_n("C1", 3.0)
    );
    lk!(
        r,
        s,
        "INDEX",
        [reference(s, &["A1:C1"]), omitted(), num("2")],
        at_n("B1", 2.0)
    );
    lk!(
        r,
        s,
        "INDEX",
        [reference(s, &["B1:B4"]), num("4")],
        at_n("B4", 0.0)
    );
    lk_shape!(
        r,
        s,
        "INDEX",
        [grid(s), num("0"), num("2")],
        Gives::Area("B1:B3"),
        INDEX_AREA,
        E(Value)
    );
    lk_shape!(
        r,
        s,
        "INDEX",
        [grid(s), num("2"), num("0")],
        Gives::Area("A2:C2"),
        INDEX_AREA,
        E(Value)
    );
    lk_shape!(
        r,
        s,
        "INDEX",
        [grid(s), num("0"), num("0")],
        Gives::Area("A1:C3"),
        INDEX_AREA,
        E(Value)
    );
    lk!(
        r,
        s,
        "INDEX",
        [two_areas(s), num("2"), num("1"), num("2")],
        at_n("C2", 6.0)
    );
    lk!(r, s, "INDEX", [two_areas(s), num("3")], at_n("A3", 7.0));
    lk!(
        r,
        s,
        "INDEX",
        [reference(s, &["A:A"]), num("1000")],
        Gives::Cell("A1000", t("kz"))
    );
    lk!(
        r,
        s,
        "INDEX",
        [grid(s), txt("2"), txt("3")],
        at_n("C2", 6.0)
    );
    lk!(r, s, "INDEX", [grid(s), num("4"), num("1")], er(Ref));
    lk!(
        r,
        s,
        "INDEX",
        [grid(s), num("1"), num("1"), num("2")],
        er(Ref)
    );
    lk!(r, s, "INDEX", [grid(s), num("2")], er(Ref));
    lk!(r, s, "INDEX", [grid(s), num("-1"), num("1")], er(Value));
    lk!(
        r,
        s,
        "INDEX",
        [grid(s), err(NA), err(WorksheetErrorCode::Div0)],
        er(NA)
    );

    // The reference reaches a reference parameter as a reference: SUM, ROWS, COLUMNS, AREAS,
    // ROW, COLUMN and COUNTBLANK read or measure the area INDEX selected.
    let column_two = nest!(s, "INDEX", [grid(s), num("0"), num("2")]);
    lk!(r, s, "SUM", [column_two.clone()], n(15.0));
    let row_two = nest!(s, "INDEX", [grid(s), num("2"), num("0")]);
    lk!(r, s, "SUM", [row_two.clone()], n(15.0));
    let whole = nest!(s, "INDEX", [grid(s), num("0"), num("0")]);
    lk!(r, s, "SUM", [whole], n(45.0));
    lk!(r, s, "ROWS", [column_two], n(3.0));
    lk!(r, s, "COLUMNS", [row_two], n(3.0));
    let picked = nest!(s, "INDEX", [two_areas(s), num("1"), num("1"), num("2")]);
    lk!(r, s, "AREAS", [picked], n(1.0));
    let cell_c2 = nest!(s, "INDEX", [grid(s), num("2"), num("3")]);
    lk!(r, s, "ROW", [cell_c2.clone()], n(2.0));
    lk!(r, s, "COLUMN", [cell_c2], n(3.0));
    let row_four = nest!(s, "INDEX", [reference(s, &["A1:C4"]), num("4"), num("0")]);
    lk!(r, s, "COUNTBLANK", [row_four], n(1.0));

    // The whole-column shape INDEX(range, MATCH(key, column, 0)).
    let at_k3 = nest!(s, "MATCH", [txt("k3"), reference(s, &["A:A"]), num("0")]);
    lk!(
        r,
        s,
        "INDEX",
        [reference(s, &["B:B"]), at_k3],
        at_n("B12", 30.0)
    );
    let at_k5 = nest!(s, "MATCH", [txt("K5"), reference(s, &["A:A"]), num("0")]);
    lk!(
        r,
        s,
        "INDEX",
        [reference(s, &["B:B"]), at_k5],
        at_n("B14", 50.0)
    );
    let at_kz = nest!(s, "MATCH", [txt("kz"), reference(s, &["A:A"]), num("0")]);
    lk!(
        r,
        s,
        "INDEX",
        [reference(s, &["C:C"]), at_kz],
        at_n("C1000", 77.0)
    );
    let at_k4 = nest!(s, "MATCH", [txt("k4"), reference(s, &["A:A"]), num("0")]);
    lk!(
        r,
        s,
        "INDEX",
        [reference(s, &["B:B"]), at_k4],
        at_n("B13", 0.0)
    );
    let at_k9 = nest!(s, "MATCH", [txt("k9"), reference(s, &["A:A"]), num("0")]);
    lk!(r, s, "INDEX", [reference(s, &["B:B"]), at_k9], er(NA));

    // XLOOKUP and CHOOSE return what they select as it is: a reference stays a reference.
    let keys = |s: &TestSheet| reference(s, &["A10:A14"]);
    let values = |s: &TestSheet| reference(s, &["B10:B14"]);
    lk!(
        r,
        s,
        "XLOOKUP",
        [txt("k3"), keys(s), values(s)],
        at_n("B12", 30.0)
    );
    lk!(
        r,
        s,
        "XLOOKUP",
        [txt("k4"), keys(s), values(s)],
        at_n("B13", 0.0)
    );
    lk!(
        r,
        s,
        "XLOOKUP",
        [txt("k9"), keys(s), values(s), reference(s, &["C4"])],
        Gives::Cell("C4", L(true))
    );
    let row_k2 = nest!(
        s,
        "XLOOKUP",
        [txt("k2"), keys(s), reference(s, &["B10:C14"])]
    );
    lk!(r, s, "SUM", [row_k2], n(20.0));
    lk!(
        r,
        s,
        "CHOOSE",
        [num("2"), reference(s, &["A1"]), reference(s, &["B1"])],
        at_n("B1", 2.0)
    );
    let chosen = nest!(
        s,
        "CHOOSE",
        [num("2"), reference(s, &["A1:A3"]), reference(s, &["B1:B3"])]
    );
    lk!(r, s, "SUM", [chosen], n(15.0));
    let first = nest!(
        s,
        "CHOOSE",
        [num("1"), reference(s, &["A1:A3"]), reference(s, &["B1:B2"])]
    );
    lk!(r, s, "ROWS", [first], n(3.0));
    lk!(
        r,
        s,
        "CHOOSE",
        [num("2"), txt("a"), reference(s, &["B4"])],
        at_n("B4", 0.0)
    );

    // VLOOKUP and INDEX of an array give values, not references.
    lk!(
        r,
        s,
        "VLOOKUP",
        [
            txt("k3"),
            reference(s, &["A10:C14"]),
            num("3"),
            logical(false)
        ],
        tx("x30")
    );
    lk_shape!(
        r,
        s,
        "VLOOKUP",
        [
            txt("k4"),
            reference(s, &["A10:B14"]),
            num("2"),
            logical(false)
        ],
        Gives::Blank,
        BLANK_RESULT,
        N(0.0)
    );
    let square = || {
        array_arg(
            "{1,2;3,4}",
            vec![
                vec![CalcValue::number(1.0), CalcValue::number(2.0)],
                vec![CalcValue::number(3.0), CalcValue::number(4.0)],
            ],
        )
    };
    lk!(r, s, "INDEX", [square(), num("2"), num("1")], n(3.0));
    lk_shape!(
        r,
        s,
        "INDEX",
        [square(), num("0"), num("2")],
        Gives::Array(vec![vec![2.0], vec![4.0]]),
        INDEX_ARRAY,
        N(2.0)
    );
    let second_row = nest!(s, "INDEX", [square(), num("2")]);
    lk!(r, s, "SUM", [second_row], n(7.0));
    lk!(r, s, "INDEX", [num("5"), num("1")], n(5.0));
    lk!(
        r,
        s,
        "AREAS",
        [reference(s, &["A1:B2", "C3", "A4"])],
        n(3.0)
    );
    lk!(r, s, "ROWS", [reference(s, &["A:A"])], n(1_048_576.0));

    // What a cell shows of the results above that it does not show as they are: INDEX of a
    // reference with a 0 index is a reference (AREAS of a value that is not one is #VALUE!), INDEX
    // of an array with a 0 index the array of the column (its ROWS and SUM), and a blank result
    // joins & as empty text.
    let column_two = nest!(s, "INDEX", [grid(s), num("0"), num("2")]);
    lk!(r, s, "AREAS", [column_two], n(1.0));
    let array_column = nest!(s, "INDEX", [square(), num("0"), num("2")]);
    lk!(r, s, "ROWS", [array_column.clone()], n(2.0));
    lk!(r, s, "SUM", [array_column], n(6.0));
    let blank_cell = nest!(
        s,
        "VLOOKUP",
        [
            txt("k4"),
            reference(s, &["A10:B14"]),
            num("2"),
            logical(false)
        ]
    );
    lookup_row(
        r,
        s,
        AMPERSAND,
        &[blank_cell, txt("")],
        tx(""),
        Seen::Measured,
    );

    // INDEX's rules that earlier rounds read from Excel ad hoc only: area_num 0 or given empty
    // #VALUE!, above 1 for an array or a single value #REF!; a logical index, a fractional one
    // truncated, a negative one #VALUE! and a huge one #REF!; column_num omitted on a one-row and
    // a one-column array; column_num given empty selects the row, omitted on a grid #REF!.
    lk!(
        r,
        s,
        "INDEX",
        [two_areas(s), num("1"), num("1"), num("0")],
        er(Value)
    );
    lk!(
        r,
        s,
        "INDEX",
        [two_areas(s), num("1"), num("1"), omitted()],
        er(Value)
    );
    lk!(
        r,
        s,
        "INDEX",
        [square(), num("1"), num("1"), num("2")],
        er(Ref)
    );
    lk!(
        r,
        s,
        "INDEX",
        [num("5"), num("1"), num("1"), num("2")],
        er(Ref)
    );
    lk!(
        r,
        s,
        "INDEX",
        [grid(s), logical(true), num("1")],
        at_n("A1", 1.0)
    );
    lk!(r, s, "INDEX", [grid(s), num("-0.5"), num("1")], er(Value));
    lk!(r, s, "INDEX", [grid(s), num("1E+300"), num("1")], er(Ref));
    let line = |excel: &str, along_row: bool| {
        let cells = [1.0, 2.0, 3.0].map(CalcValue::number).to_vec();
        let rows = if along_row {
            vec![cells]
        } else {
            cells.into_iter().map(|cell| vec![cell]).collect()
        };
        array_arg(excel, rows)
    };
    lk!(r, s, "INDEX", [line("{1,2,3}", true), num("2")], n(2.0));
    lk!(r, s, "INDEX", [line("{1;2;3}", false), num("2")], n(2.0));
    lk!(
        r,
        s,
        "INDEX",
        [grid(s), num("1.9999"), num("1")],
        at_n("A1", 1.0)
    );
    let row_given_empty = nest!(s, "INDEX", [grid(s), num("2"), omitted()]);
    lk!(r, s, "SUM", [row_given_empty], n(15.0));
    let row_omitted = nest!(s, "INDEX", [grid(s), num("2")]);
    lk!(r, s, "SUM", [row_omitted], er(Ref));
    rows
}

#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test)]
fn cfunc_m3_6__reference_result() {
    const CASE: &str = "cfunc_m3_6__reference_result";
    let fast = NumericMode::Fast;
    let mut sheet = reference_result_sheet();
    note!(
        "ACCEPT-NOTE: {CASE}: the sheet holds {}; every other cell is blank",
        sheet.described()
    );
    let rows = reference_result_rows(&mut sheet, false);
    let s = &mut sheet;

    // INDEX's result is a CoreValue::Reference to the cell it selects, and a value parameter
    // refuses it at admission: the caller dereferences a reference before a value parameter, as
    // calc-fml does (dc-l10u judges that route against the capture).
    let index = surface(Notation::Call, "INDEX");
    let selected = invoke(
        index,
        &mut Recorded::new(vec![
            reference(s, &["A1:C3"]).value,
            CalcValue::number(2.0),
            CalcValue::number(2.0),
        ]),
        &mut EvalServices::with_cells(&fast, s),
        &mut FunctionCallScratch::new(),
    )
    .expect("INDEX(A1:C3,2,2) computes");
    assert!(
        matches!(selected.core(), CoreValue::Reference(reference) if *reference == ReferenceLike::cell(s.key("B2"))),
        "INDEX(A1:C3,2,2) gives {selected:?}"
    );
    let plus = surface(Notation::Infix, "+");
    assert_eq!(
        run(
            plus,
            &mut Recorded::new(vec![selected.clone(), CalcValue::number(0.0)]),
            &fast
        ),
        Err(FunctionFault::ReferenceArgument {
            id: plus.id(),
            index: 0
        })
    );
    note!(
        "ACCEPT-NOTE: {CASE}: INDEX(A1:C3,2,2) through calc_func::invoke gives a CoreValue::Reference to B2; the + operator refuses it at its value parameter with ReferenceArgument, so a consumer must dereference it first (calc_fml's route, judged under dc-l10u)"
    );

    // INDEX reads no cell: it computes a reference from its argument's areas alone. MATCH over a
    // whole column asks the reader for that column once and stops at the first match, so its work
    // follows the held cells it passes, not the column's 1,048,576 cells.
    s.reads.clear();
    s.visited = 0;
    let whole_b = reference(s, &["B:B"]).value;
    let whole_a = reference(s, &["A:A"]).value;
    let (found, bytes) = bytes_allocated_during(|| {
        let at = invoke(
            surface(Notation::Call, "MATCH"),
            &mut Recorded::new(vec![
                text_value("k3"),
                whole_a.clone(),
                CalcValue::number(0.0),
            ]),
            &mut EvalServices::with_cells(&fast, s),
            &mut FunctionCallScratch::new(),
        )
        .expect("MATCH computes");
        black_box(invoke(
            index,
            &mut Recorded::new(vec![whole_b.clone(), at]),
            &mut EvalServices::with_cells(&fast, s),
            &mut FunctionCallScratch::new(),
        ))
    });
    assert_eq!(
        found.as_ref().ok().and_then(CalcValue::as_reference),
        Some(&ReferenceLike::cell(s.key("B12")))
    );
    let column_a = s.area("A:A");
    assert_eq!(s.reads, [column_a], "the reader is asked for column A once");
    // A1:A4 and A10:A12 lie before the match; the walk stops at A12.
    assert_eq!(s.visited, 7, "the cells the reader visited");
    assert!(
        bytes < SPARSE_BYTES,
        "INDEX(B:B,MATCH(\"k3\",A:A,0)) allocated {bytes} bytes"
    );
    note!(
        "ACCEPT-NOTE: {CASE}: INDEX(B:B,MATCH(\"k3\",A:A,0)) through calc_func::invoke gives a reference to B12; the instrumented reader was asked for A1:A1048576 once and visited {} cells of its {} before the match stopped the walk; INDEX read no cell; {bytes} bytes allocated",
        s.visited,
        column_a.cell_count()
    );

    // XLOOKUP's and CHOOSE's results are the references they select, by identity.
    let chosen = invoke(
        surface(Notation::Call, "CHOOSE"),
        &mut Recorded::new(vec![
            CalcValue::number(2.0),
            reference(s, &["A1:A3"]).value,
            reference(s, &["B1:B3"]).value,
        ]),
        &mut EvalServices::with_cells(&fast, s),
        &mut FunctionCallScratch::new(),
    );
    assert_eq!(chosen, Ok(reference(s, &["B1:B3"]).value));
    let found_row = invoke(
        surface(Notation::Call, "XLOOKUP"),
        &mut Recorded::new(vec![
            text_value("k2"),
            reference(s, &["A10:A14"]).value,
            reference(s, &["B10:C14"]).value,
        ]),
        &mut EvalServices::with_cells(&fast, s),
        &mut FunctionCallScratch::new(),
    );
    assert_eq!(found_row, Ok(reference(s, &["B11:C11"]).value));
    note!(
        "ACCEPT-NOTE: {CASE}: CHOOSE(2,A1:A3,B1:B3) gives the reference B1:B3 and XLOOKUP(\"k2\",A10:A14,B10:C14) the reference B11:C11"
    );

    assert_lookup_workbook(&rows, s);
    lookup_rows_note(&rows);
}

// ---------------------------------------------------------------------------------------------
// cfunc_m3_6__lookup_edges

/// Every ordered pair of two different errors of the eight that a cell constant can hold (the
/// fixtures' error literals, in XMATCH's order of them), by first error and then second.
fn error_pairs() -> Vec<(WorksheetErrorCode, WorksheetErrorCode)> {
    let written: Vec<WorksheetErrorCode> = ERROR_LITERALS.iter().map(|(code, _)| *code).collect();
    let mut pairs = Vec::new();
    for first in &written {
        for second in &written {
            if first != second {
                pairs.push((*first, *second));
            }
        }
    }
    pairs
}

/// The cells the edge rows read. A1:A6 hold 1, 2, 2, 2, 3, 5 (ascending, a run of equal values);
/// B1:B6 fruit names, two equal ignoring case; C1:C6 10 to 60. A7:A12 hold 5, 4, 3, 3, 2, 1
/// (descending); B7:B12 texts with wildcard characters and the text "1", then the number 1;
/// C7:C12 one value of each kind (C9 is blank). A13:C15 is a table laid out by columns. A16:A20
/// hold unsorted numbers, B16:B20 texts (B16 empty text), C16:C20 nearly equal numbers. A21:A37,
/// B21:B32 and C21:C26 hold three lines of mixed kinds from the measured sweep. A40:A42 hold -1,
/// 0 and 1, and C30:C34 three errors, TRUE and 7. Columns F to R hold the cells of the rows
/// written from the legacy tests and the error pairs, with the source formulas in P and Q, as the
/// code below describes.
fn lookup_edges_sheet() -> TestSheet {
    use WorksheetErrorCode::{Div0, NA, Num};
    let num = |v: f64| CalcValue::number(v);
    let text = text_value;
    let b = CalcValue::logical;
    let e = CalcValue::error;
    let mut cells: Vec<(String, String, CalcValue)> = Vec::new();
    let mut put = |a1: &str, entry: &str, value: CalcValue| {
        cells.push((a1.to_owned(), entry.to_owned(), value));
    };
    for (row, v) in [1.0, 2.0, 2.0, 2.0, 3.0, 5.0].into_iter().enumerate() {
        put(&format!("A{}", row + 1), &format!("{v}"), num(v));
    }
    for (row, word) in ["apple", "Banana", "banana", "cherry", "date", "fig"]
        .into_iter()
        .enumerate()
    {
        put(
            &format!("B{}", row + 1),
            &format!("text {word:?}"),
            text(word),
        );
    }
    for row in 0..6 {
        let v = 10.0 * (row + 1) as f64;
        put(&format!("C{}", row + 1), &format!("{v}"), num(v));
    }
    for (row, v) in [5.0, 4.0, 3.0, 3.0, 2.0, 1.0].into_iter().enumerate() {
        put(&format!("A{}", row + 7), &format!("{v}"), num(v));
    }
    for (row, word) in ["a*b", "a?c", "~", "x~y", "1"].into_iter().enumerate() {
        put(
            &format!("B{}", row + 7),
            &format!("text {word:?}"),
            text(word),
        );
    }
    put("B12", "1", num(1.0));
    put("C7", "3", num(3.0));
    put("C8", "text \"a\"", text("a"));
    put("C10", "TRUE", b(true));
    put("C11", "#N/A", e(NA));
    put("C12", "3", num(3.0));
    for (column, (key, word, v)) in [(1.0, "p", 10.0), (2.0, "q", 20.0), (3.0, "r", 30.0)]
        .into_iter()
        .enumerate()
    {
        let letter = ["A", "B", "C"][column];
        put(&format!("{letter}13"), &format!("{key}"), num(key));
        put(
            &format!("{letter}14"),
            &format!("text {word:?}"),
            text(word),
        );
        put(&format!("{letter}15"), &format!("{v}"), num(v));
    }
    for (row, v) in [3.0, 1.0, 4.0, 1.0, 5.0].into_iter().enumerate() {
        put(&format!("A{}", row + 16), &format!("{v}"), num(v));
    }
    for (row, word) in ["", "ab", "a", "a*c", "abc"].into_iter().enumerate() {
        put(
            &format!("B{}", row + 16),
            &format!("text {word:?}"),
            text(word),
        );
    }
    for (row, v) in [
        0.3,
        0.300_000_000_000_000_04,
        1.0,
        1.000_000_000_000_000_2,
        0.1,
    ]
    .into_iter()
    .enumerate()
    {
        put(&format!("C{}", row + 16), &format!("{v:?}"), num(v));
    }
    // Three lines of the measured sweep, in A21:A37, B21:B32 and C21:C26 (None is blank).
    #[derive(Clone, Copy)]
    enum K {
        N(f64),
        T(&'static str),
        B(bool),
        E,
        Z,
    }
    let lines: [(&str, u32, &[K]); 3] = [
        (
            "A",
            21,
            &[
                K::Z,
                K::N(1.0),
                K::T("d"),
                K::T("a"),
                K::T("a"),
                K::Z,
                K::N(1.0),
                K::N(5.0),
                K::N(4.0),
                K::B(false),
                K::N(2.0),
                K::N(5.0),
                K::N(2.0),
                K::T("d"),
                K::N(5.0),
                K::T("c"),
                K::N(3.0),
            ],
        ),
        (
            "B",
            21,
            &[
                K::T("c"),
                K::T("a"),
                K::N(5.0),
                K::N(2.0),
                K::N(3.0),
                K::N(3.0),
                K::N(5.0),
                K::N(1.0),
                K::N(1.0),
                K::E,
                K::N(1.0),
                K::N(3.0),
            ],
        ),
        (
            "C",
            21,
            &[
                K::N(4.0),
                K::N(1.0),
                K::T("d"),
                K::N(1.0),
                K::N(5.0),
                K::B(true),
            ],
        ),
    ];
    for (letter, start, line) in lines {
        for (offset, k) in line.iter().enumerate() {
            let a1 = format!("{letter}{}", start + offset as u32);
            match *k {
                K::N(v) => put(&a1, &format!("{v}"), num(v)),
                K::T(word) => put(&a1, &format!("text {word:?}"), text(word)),
                K::B(v) => put(&a1, if v { "TRUE" } else { "FALSE" }, b(v)),
                K::E => put(&a1, "#N/A", e(NA)),
                K::Z => {}
            }
        }
    }
    put("C30", "#DIV/0!", e(Div0));
    put("C31", "#N/A", e(NA));
    put("C32", "#NUM!", e(Num));
    put("C33", "TRUE", b(true));
    put("C34", "7", num(7.0));
    put("A40", "-1", num(-1.0));
    put("A41", "0", num(0.0));
    put("A42", "1", num(1.0));
    // F1:G56: every ordered pair of two different errors of the eight a cell constant can hold.
    for (row, (first, second)) in error_pairs().into_iter().enumerate() {
        put(&format!("F{}", row + 1), spelled(first), e(first));
        put(&format!("G{}", row + 1), spelled(second), e(second));
    }
    // Lines with a blank cell: H1:H4 is 1, 2, blank, 3; H6:H9 1, blank, 2, 3; H11:H14 3, blank,
    // 2, 1.
    for (a1, v) in [
        ("H1", 1.0),
        ("H2", 2.0),
        ("H4", 3.0),
        ("H6", 1.0),
        ("H8", 2.0),
        ("H9", 3.0),
        ("H11", 3.0),
        ("H13", 2.0),
        ("H14", 1.0),
    ] {
        put(a1, &format!("{v}"), num(v));
    }
    // I1:I3 is blank, empty text and 1; I5:J5 blank and 1; I7:J7 empty text and blank; I9:J9
    // blank and 20.
    put("I2", "text \"\"", text(""));
    put("I3", "1", num(1.0));
    put("J5", "1", num(1.0));
    put("I7", "text \"\"", text(""));
    put("J9", "20", num(20.0));
    // K1 and K2: two numbers that differ in their 16th significant digit.
    for (a1, v) in [
        ("K1", 1_234_567_890_123_455.0 / 1.0e25),
        ("K2", 1_234_567_890_123_454.0 / 1.0e25),
    ] {
        put(a1, &format!("{v:?}"), num(v));
    }
    // L1:M4: a table whose first column holds 1, 2, a blank cell and 3, beside 10, 20, 99 and 30;
    // L6:O7 the same table across.
    for (row, (key, v)) in [
        (Some(1.0), 10.0),
        (Some(2.0), 20.0),
        (None, 99.0),
        (Some(3.0), 30.0),
    ]
    .into_iter()
    .enumerate()
    {
        let column = ["L", "M", "N", "O"][row];
        if let Some(key) = key {
            put(&format!("L{}", row + 1), &format!("{key}"), num(key));
            put(&format!("{column}6"), &format!("{key}"), num(key));
        }
        put(&format!("M{}", row + 1), &format!("{v}"), num(v));
        put(&format!("{column}7"), &format!("{v}"), num(v));
    }
    // P1:Q54: every ordered pair of two different errors of the eleven XMATCH orders with
    // #SPILL!, #FIELD! or #CALC! among them. A cell of one of those three holds its source formula
    // (lookup_sources), for which the reader holds the error Excel gives it; the others hold a
    // constant. R holds the text that blocks the spill of a SEQUENCE in Q (Q blocks one in P).
    let sources = lookup_sources("lookup_edges");
    for (row, (first, second)) in source_error_pairs().into_iter().enumerate() {
        for (column, code) in [("P", first), ("Q", second)] {
            let a1 = format!("{column}{}", row + 1);
            match sources.iter().find(|source| source.a1 == a1) {
                Some(source) => put(
                    &a1,
                    &format!(
                        "formula ={} (a dynamic-array formula), for which the reader holds {}",
                        source.entered,
                        error_name(code)
                    ),
                    e(code),
                ),
                None => put(&a1, spelled(code), e(code)),
            }
        }
        if second == WorksheetErrorCode::Spill {
            put(&format!("R{}", row + 1), "text \"block\"", text("block"));
        }
    }
    let borrowed: Vec<(&str, &str, CalcValue)> = cells
        .iter()
        .map(|(a1, entry, value)| (a1.as_str(), entry.as_str(), value.clone()))
        .collect();
    TestSheet::new(&borrowed)
}

/// The rows of cfunc_m3_6__lookup_edges, in the order the lookup oracle workbook's sheet
/// `lookup_edges` holds their formulas.
fn lookup_edges_rows(s: &mut TestSheet, quiet: bool) -> Rows {
    const CASE: &str = "cfunc_m3_6__lookup_edges";
    let mut rows = Rows::new(CASE, "lookup_edges", quiet);
    let r = &mut rows;
    use WorksheetErrorCode::{Div0, NA, Ref, Value};
    let rf = |s: &TestSheet, a: &str| reference(s, &[a]);
    let asc = |s: &TestSheet| reference(s, &["A1:A6"]);
    let fruit = |s: &TestSheet| reference(s, &["B1:B6"]);
    let desc = |s: &TestSheet| reference(s, &["A7:A12"]);
    let marks = |s: &TestSheet| reference(s, &["B7:B12"]);
    let mixed = |s: &TestSheet| reference(s, &["C7:C12"]);
    let tens = |s: &TestSheet| reference(s, &["C1:C6"]);
    let table = |s: &TestSheet| reference(s, &["A1:C6"]);
    let wide = |s: &TestSheet| reference(s, &["A13:C15"]);
    let signs = |s: &TestSheet| reference(s, &["A40:A42"]);

    // MATCH: an exact match is the first equal cell; the bisection ends at the last of a run of
    // equal cells, below the first cell is #N/A and above the last the last; match_type -1 walks
    // a descending line and stops at the first smaller cell.
    lk!(r, s, "MATCH", [num("2"), asc(s), num("0")], n(2.0));
    lk!(r, s, "MATCH", [num("2"), asc(s), num("1")], n(4.0));
    lk!(r, s, "MATCH", [num("2"), asc(s)], n(4.0));
    lk!(r, s, "MATCH", [num("2.5"), asc(s), num("1")], n(4.0));
    lk!(r, s, "MATCH", [num("0"), asc(s), num("1")], er(NA));
    lk!(r, s, "MATCH", [num("9"), asc(s), num("1")], n(6.0));
    lk!(r, s, "MATCH", [num("3"), desc(s), num("-1")], n(3.0));
    lk!(r, s, "MATCH", [num("3.5"), desc(s), num("-1")], n(2.0));
    lk!(r, s, "MATCH", [num("6"), desc(s), num("-1")], er(NA));
    lk!(r, s, "MATCH", [num("0"), desc(s), num("-1")], n(6.0));
    lk!(r, s, "MATCH", [num("3"), desc(s), num("1")], n(4.0));
    lk!(r, s, "MATCH", [num("2"), asc(s), num("-1")], er(NA));
    // Text: ignoring case, with wildcards in an exact match only.
    lk!(r, s, "MATCH", [txt("BANANA"), fruit(s), num("0")], n(2.0));
    lk!(r, s, "MATCH", [txt("BANANA"), fruit(s), num("1")], n(3.0));
    lk!(r, s, "MATCH", [txt("b*"), fruit(s), num("0")], n(2.0));
    lk!(r, s, "MATCH", [txt("?pple"), fruit(s), num("0")], n(1.0));
    lk!(r, s, "MATCH", [txt("c"), fruit(s), num("1")], n(3.0));
    lk!(r, s, "MATCH", [txt("a"), fruit(s), num("1")], er(NA));
    lk!(r, s, "MATCH", [txt("a~*b"), marks(s), num("0")], n(1.0));
    lk!(r, s, "MATCH", [txt("x~y"), marks(s), num("0")], er(NA));
    lk!(r, s, "MATCH", [txt("~~"), marks(s), num("0")], n(3.0));
    // Kinds never mix: number text is text, and a blank cell is no value.
    lk!(r, s, "MATCH", [txt("1"), marks(s), num("0")], n(5.0));
    lk!(r, s, "MATCH", [num("1"), marks(s), num("0")], n(6.0));
    lk!(r, s, "MATCH", [txt("2"), asc(s), num("0")], er(NA));
    lk!(r, s, "MATCH", [logical(true), mixed(s), num("0")], n(4.0));
    lk!(r, s, "MATCH", [txt("*"), mixed(s), num("0")], n(2.0));
    lk!(r, s, "MATCH", [num("0"), mixed(s), num("0")], er(NA));
    lk!(r, s, "MATCH", [num("4"), mixed(s), num("1")], n(6.0));
    lk!(r, s, "MATCH", [txt("b"), mixed(s), num("1")], n(2.0));
    // match_type: only its sign counts; omitted is 1, given empty 0.
    lk!(r, s, "MATCH", [num("2"), asc(s), num("2")], n(4.0));
    lk!(r, s, "MATCH", [num("2"), asc(s), num("-0.5")], er(NA));
    lk!(r, s, "MATCH", [num("2"), asc(s), txt("0")], n(2.0));
    lk!(r, s, "MATCH", [num("2"), asc(s), omitted()], n(2.0));
    // The lookup array: one row or column of a reference or an array.
    lk!(r, s, "MATCH", [num("2"), rf(s, "A1:B6"), num("0")], er(NA));
    lk!(r, s, "MATCH", [num("2"), num("2"), num("0")], er(NA));
    lk!(
        r,
        s,
        "MATCH",
        [
            num("2"),
            array_arg(
                "{1,2,3}",
                vec![vec![
                    CalcValue::number(1.0),
                    CalcValue::number(2.0),
                    CalcValue::number(3.0)
                ]]
            ),
            num("0")
        ],
        n(2.0)
    );
    // Numbers match by their exact binary64 value.
    let near = |s: &TestSheet| reference(s, &["C16:C20"]);
    lk!(r, s, "MATCH", [cell(s, "C17"), near(s), num("0")], n(2.0));
    lk!(
        r,
        s,
        "MATCH",
        [num("0.3"), rf(s, "C17:C20"), num("0")],
        er(NA)
    );
    lk!(
        r,
        s,
        "MATCH",
        [cell(s, "C19"), rf(s, "C18:C19"), num("0")],
        n(2.0)
    );
    // A text lookup value of more than 255 code units, and argument errors in order.
    lk!(
        r,
        s,
        "MATCH",
        [txt(&"a".repeat(256)), fruit(s), num("0")],
        er(Value)
    );
    lk!(r, s, "MATCH", [err(NA), asc(s), txt("x")], er(NA));
    lk!(r, s, "MATCH", [num("2"), err(Ref), err(NA)], er(Ref));
    // A blank or omitted lookup value is the number 0.
    lk!(r, s, "MATCH", [omitted(), signs(s), num("0")], n(2.0));
    lk!(r, s, "MATCH", [cell(s, "C9"), signs(s), num("1")], n(2.0));
    // Unsorted lines of mixed kinds, from the measured sweep.
    lk!(
        r,
        s,
        "MATCH",
        [txt("c"), rf(s, "A21:A37"), num("1")],
        er(NA)
    );
    lk!(
        r,
        s,
        "MATCH",
        [txt("bb"), rf(s, "B21:B32"), num("1")],
        er(NA)
    );
    lk!(
        r,
        s,
        "MATCH",
        [num("2.5"), rf(s, "C21:C26"), num("-1")],
        n(1.0)
    );
    lk!(
        r,
        s,
        "MATCH",
        [num("2.5"), rf(s, "C21:C26"), num("1")],
        n(4.0)
    );

    // XMATCH: linear searches from either end, binary searches, and the order of every kind.
    lk!(r, s, "XMATCH", [num("2"), asc(s)], n(2.0));
    lk!(
        r,
        s,
        "XMATCH",
        [num("2"), asc(s), num("0"), num("-1")],
        n(4.0)
    );
    lk!(r, s, "XMATCH", [num("2.5"), asc(s), num("-1")], n(2.0));
    lk!(
        r,
        s,
        "XMATCH",
        [num("2.5"), asc(s), num("-1"), num("-1")],
        n(4.0)
    );
    lk!(r, s, "XMATCH", [num("2.5"), asc(s), num("1")], n(5.0));
    lk!(r, s, "XMATCH", [num("0"), asc(s), num("-1")], er(NA));
    lk!(r, s, "XMATCH", [num("9"), asc(s), num("1")], er(NA));
    lk!(
        r,
        s,
        "XMATCH",
        [num("2"), asc(s), num("0"), num("2")],
        n(2.0)
    );
    lk!(
        r,
        s,
        "XMATCH",
        [num("3"), desc(s), num("0"), num("-2")],
        n(4.0)
    );
    lk!(
        r,
        s,
        "XMATCH",
        [num("2.5"), asc(s), num("-1"), num("2")],
        n(4.0)
    );
    lk!(r, s, "XMATCH", [txt("b*"), fruit(s), num("2")], n(2.0));
    lk!(r, s, "XMATCH", [txt("b*"), fruit(s), num("0")], er(NA));
    lk!(
        r,
        s,
        "XMATCH",
        [txt("banana"), fruit(s), num("0"), num("-1")],
        n(3.0)
    );
    lk!(
        r,
        s,
        "XMATCH",
        [num("2"), asc(s), num("0"), num("0")],
        er(Value)
    );
    lk!(r, s, "XMATCH", [num("2"), asc(s), num("-1.5")], er(Value));
    lk!(r, s, "XMATCH", [num("2.5"), asc(s), num("-0.5")], n(2.0));
    lk!(
        r,
        s,
        "XMATCH",
        [txt("b*"), fruit(s), num("2"), num("2")],
        er(Value)
    );
    lk!(r, s, "XMATCH", [num("2"), rf(s, "A1:B6")], er(Value));
    lk!(r, s, "XMATCH", [num("2"), num("2")], n(1.0));
    lk!(r, s, "XMATCH", [txt("2"), asc(s)], er(NA));
    lk!(r, s, "XMATCH", [cell(s, "C9"), mixed(s)], n(3.0));
    lk!(r, s, "XMATCH", [txt(""), mixed(s)], er(NA));
    lk!(r, s, "XMATCH", [cell(s, "C9"), signs(s), num("-1")], n(3.0));
    lk!(r, s, "XMATCH", [cell(s, "C9"), signs(s), num("1")], er(NA));
    let errors = |s: &TestSheet| reference(s, &["C30:C34"]);
    lk!(r, s, "XMATCH", [num("6"), errors(s), num("1")], n(5.0));
    lk!(r, s, "XMATCH", [num("8"), errors(s), num("1")], n(4.0));
    lk!(
        r,
        s,
        "XMATCH",
        [num("8"), rf(s, "C30:C32"), num("1")],
        n(1.0)
    );
    lk!(
        r,
        s,
        "XMATCH",
        [num("2.5"), rf(s, "C21:C26"), num("1"), num("-2")],
        n(6.0)
    );
    lk!(
        r,
        s,
        "XMATCH",
        [num("99"), rf(s, "A40:A45"), num("1")],
        n(4.0)
    );
    lk!(
        r,
        s,
        "XMATCH",
        [num("99"), rf(s, "A40:A45"), num("1"), num("-1")],
        n(6.0)
    );
    lk!(
        r,
        s,
        "XMATCH",
        [num("2.5"), rf(s, "C21:C26"), num("-1")],
        n(2.0)
    );
    lk!(
        r,
        s,
        "XMATCH",
        [num("2.5"), rf(s, "C21:C26"), num("-1"), num("-1")],
        n(4.0)
    );

    // XLOOKUP: the row of the return array at the match, if_not_found, and shapes.
    lk!(
        r,
        s,
        "XLOOKUP",
        [num("2"), asc(s), tens(s)],
        at_n("C2", 20.0)
    );
    lk!(
        r,
        s,
        "XLOOKUP",
        [num("2"), asc(s), tens(s), omitted(), num("0"), num("-1")],
        at_n("C4", 40.0)
    );
    lk!(r, s, "XLOOKUP", [num("7"), asc(s), tens(s)], er(NA));
    lk!(
        r,
        s,
        "XLOOKUP",
        [num("7"), asc(s), tens(s), txt("none")],
        tx("none")
    );
    lk!(
        r,
        s,
        "XLOOKUP",
        [num("7"), asc(s), tens(s), omitted()],
        er(NA)
    );
    lk!(
        r,
        s,
        "XLOOKUP",
        [num("2.5"), asc(s), tens(s), omitted(), num("-1")],
        at_n("C2", 20.0)
    );
    lk!(
        r,
        s,
        "XLOOKUP",
        [num("2.5"), asc(s), tens(s), omitted(), num("1")],
        at_n("C5", 50.0)
    );
    lk!(
        r,
        s,
        "XLOOKUP",
        [num("3"), asc(s), rf(s, "C1:C5")],
        er(Value)
    );
    lk!(r, s, "XLOOKUP", [err(NA), asc(s), rf(s, "C1:C5")], er(NA));
    lk!(
        r,
        s,
        "XLOOKUP",
        [num("2"), rf(s, "A13:C13"), rf(s, "A15:C15")],
        at_n("B15", 20.0)
    );
    lk!(
        r,
        s,
        "XLOOKUP",
        [num("2"), rf(s, "A13:C13"), rf(s, "A1:A3")],
        er(Value)
    );
    lk!(
        r,
        s,
        "XLOOKUP",
        [num("7"), asc(s), tens(s), err(Div0)],
        er(Div0)
    );
    lk!(
        r,
        s,
        "XLOOKUP",
        [
            txt("BANANA"),
            fruit(s),
            tens(s),
            omitted(),
            num("0"),
            num("-1")
        ],
        at_n("C3", 30.0)
    );

    // VLOOKUP and HLOOKUP: exact and approximate, the index checked after the match.
    lk!(
        r,
        s,
        "VLOOKUP",
        [num("3"), table(s), num("2"), logical(false)],
        tx("date")
    );
    lk!(r, s, "VLOOKUP", [num("2"), table(s), num("3")], n(40.0));
    lk!(
        r,
        s,
        "VLOOKUP",
        [num("2.5"), table(s), num("3"), logical(true)],
        n(40.0)
    );
    lk!(
        r,
        s,
        "VLOOKUP",
        [num("0"), table(s), num("2"), logical(true)],
        er(NA)
    );
    lk!(
        r,
        s,
        "VLOOKUP",
        [num("2.5"), table(s), num("2"), logical(false)],
        er(NA)
    );
    lk!(
        r,
        s,
        "VLOOKUP",
        [num("2.5"), table(s), num("2"), omitted()],
        er(NA)
    );
    lk!(
        r,
        s,
        "VLOOKUP",
        [num("3"), table(s), num("4"), logical(false)],
        er(Ref)
    );
    lk!(
        r,
        s,
        "VLOOKUP",
        [num("3"), table(s), num("0"), logical(false)],
        er(Value)
    );
    lk!(
        r,
        s,
        "VLOOKUP",
        [num("99"), table(s), num("0"), logical(false)],
        er(NA)
    );
    lk!(
        r,
        s,
        "VLOOKUP",
        [num("3"), table(s), num("2.9"), logical(false)],
        tx("date")
    );
    lk!(
        r,
        s,
        "VLOOKUP",
        [num("3"), table(s), num("-2147483647"), logical(false)],
        er(Value)
    );
    lk!(
        r,
        s,
        "VLOOKUP",
        [num("3"), table(s), num("-2147483648"), logical(false)],
        er(Ref)
    );
    lk!(
        r,
        s,
        "VLOOKUP",
        [txt("b*"), rf(s, "B1:C6"), num("2"), logical(false)],
        n(20.0)
    );
    lk!(
        r,
        s,
        "VLOOKUP",
        [txt("3"), table(s), num("2"), logical(false)],
        er(NA)
    );
    lk!(
        r,
        s,
        "VLOOKUP",
        [num("3"), table(s), num("2"), txt("x")],
        er(Value)
    );
    lk!(
        r,
        s,
        "VLOOKUP",
        [num("3"), table(s), num("2"), txt("TRUE")],
        tx("date")
    );
    lk!(
        r,
        s,
        "VLOOKUP",
        [cell(s, "C9"), signs(s), num("1"), logical(false)],
        n(0.0)
    );
    lk_shape!(
        r,
        s,
        "VLOOKUP",
        [num("3"), rf(s, "A9:C10"), num("3"), logical(false)],
        Gives::Blank,
        BLANK_RESULT,
        N(0.0)
    );
    lk!(
        r,
        s,
        "VLOOKUP",
        [num("2.5"), rf(s, "C21:C26"), num("1"), logical(true)],
        n(1.0)
    );
    lk!(
        r,
        s,
        "HLOOKUP",
        [num("2"), wide(s), num("3"), logical(false)],
        n(20.0)
    );
    lk!(r, s, "HLOOKUP", [num("2.5"), wide(s), num("2")], tx("q"));
    lk!(
        r,
        s,
        "HLOOKUP",
        [num("3"), wide(s), num("4"), logical(false)],
        er(Ref)
    );
    lk!(
        r,
        s,
        "HLOOKUP",
        [txt("q"), rf(s, "A14:C15"), num("2"), logical(false)],
        n(20.0)
    );

    // LOOKUP: the bisection, its vector and array forms, and its result line.
    lk!(r, s, "LOOKUP", [num("3"), asc(s), tens(s)], n(50.0));
    lk!(r, s, "LOOKUP", [num("2"), asc(s), tens(s)], n(40.0));
    lk!(r, s, "LOOKUP", [num("0"), asc(s), tens(s)], er(NA));
    lk!(r, s, "LOOKUP", [num("9"), asc(s), tens(s)], n(60.0));
    lk!(
        r,
        s,
        "LOOKUP",
        [num("2"), rf(s, "A13:C13"), rf(s, "A15:C15")],
        n(20.0)
    );
    lk!(
        r,
        s,
        "LOOKUP",
        [num("2"), rf(s, "A1:A3"), rf(s, "A15:C15")],
        n(30.0)
    );
    lk!(r, s, "LOOKUP", [num("3"), table(s)], n(50.0));
    lk!(r, s, "LOOKUP", [num("2"), wide(s)], n(3.0));
    lk!(r, s, "LOOKUP", [txt("C"), fruit(s), tens(s)], n(30.0));
    lk!(r, s, "LOOKUP", [cell(s, "C9"), signs(s)], n(0.0));
    lk!(
        r,
        s,
        "LOOKUP",
        [
            num("3"),
            array_arg(
                "{1,2,3}",
                vec![vec![
                    CalcValue::number(1.0),
                    CalcValue::number(2.0),
                    CalcValue::number(3.0)
                ]]
            ),
            array_arg(
                "{\"a\",\"b\"}",
                vec![vec![text_value("a"), text_value("b")]]
            )
        ],
        er(NA)
    );
    lk!(
        r,
        s,
        "LOOKUP",
        [
            num("2"),
            array_arg(
                "{1,#DIV/0!,1,#DIV/0!}",
                vec![vec![
                    CalcValue::number(1.0),
                    CalcValue::error(Div0),
                    CalcValue::number(1.0),
                    CalcValue::error(Div0)
                ]]
            )
        ],
        n(1.0)
    );

    // CHOOSE: index_num truncated, its range, and only the chosen value asked for.
    let abc = || [txt("a"), txt("b"), txt("c")];
    let [a, b, c] = abc();
    lk!(r, s, "CHOOSE", [num("2"), a, b, c], tx("b"));
    let [a, b, c] = abc();
    lk!(r, s, "CHOOSE", [num("2.9"), a, b, c], tx("b"));
    lk!(r, s, "CHOOSE", [num("0"), txt("a"), txt("b")], er(Value));
    let [a, b, c] = abc();
    lk!(r, s, "CHOOSE", [num("4"), a, b, c], er(Value));
    lk!(r, s, "CHOOSE", [txt("2"), txt("a"), txt("b")], tx("b"));
    lk!(r, s, "CHOOSE", [logical(true), txt("a"), txt("b")], tx("a"));
    lk!(r, s, "CHOOSE", [err(NA), txt("a"), txt("b")], er(NA));
    lk_shape!(
        r,
        s,
        "CHOOSE",
        [num("2"), txt("a"), omitted()],
        Gives::Blank,
        BLANK_RESULT,
        N(0.0)
    );
    lk!(r, s, "CHOOSE", [num("2"), err(Div0), num("2")], n(2.0));

    // ROW, COLUMN, ROWS, COLUMNS and AREAS.
    lk!(r, s, "ROW", [rf(s, "B3")], n(3.0));
    lk!(r, s, "ROW", [rf(s, "B3:C3")], n(3.0));
    lk!(r, s, "COLUMN", [rf(s, "C5")], n(3.0));
    lk!(r, s, "ROW", [err(Ref)], er(Ref));
    lk!(r, s, "ROWS", [rf(s, "A1:C5")], n(5.0));
    lk!(
        r,
        s,
        "ROWS",
        [array_arg(
            "{1,2;3,4}",
            vec![
                vec![CalcValue::number(1.0), CalcValue::number(2.0)],
                vec![CalcValue::number(3.0), CalcValue::number(4.0)]
            ]
        )],
        n(2.0)
    );
    lk!(r, s, "ROWS", [num("5")], n(1.0));
    lk!(r, s, "ROWS", [txt("a")], n(1.0));
    lk!(r, s, "ROWS", [err(NA)], er(NA));
    lk!(r, s, "ROWS", [reference(s, &["A1:A3", "B1:B2"])], er(Ref));
    lk!(r, s, "COLUMNS", [rf(s, "A1:C5")], n(3.0));
    lk!(r, s, "AREAS", [rf(s, "A1")], n(1.0));

    lk_shape!(
        r,
        s,
        "ROW",
        [rf(s, "B3:B5")],
        Gives::Array(vec![vec![3.0], vec![4.0], vec![5.0]]),
        LEGACY_POSITIONS,
        N(3.0)
    );
    // A text order through cdoc-jpha's collation, in which a hyphen weighs after the letters.
    lk!(
        r,
        s,
        "MATCH",
        [txt("a-b"), rf(s, "B1:B2"), num("1")],
        er(NA)
    );
    // Refused: Excel's results here belong to other beads.
    lk_refused!(
        r,
        s,
        LIFT_OWNER,
        "MATCH",
        [
            array_arg(
                "{1,2}",
                vec![vec![CalcValue::number(1.0), CalcValue::number(2.0)]]
            ),
            asc(s),
            num("0")
        ],
        refused(Some(0))
    );
    lk!(
        r,
        s,
        "VLOOKUP",
        [num("3"), table(s), num("2"), txt("1")],
        er(Value)
    );

    // ROW and COLUMN of several areas, and of several columns.
    lk!(r, s, "ROW", [reference(s, &["A1", "B2"])], er(Ref));
    lk!(r, s, "COLUMN", [reference(s, &["A1", "B2"])], er(Ref));
    lk_shape!(
        r,
        s,
        "COLUMN",
        [rf(s, "B3:D3")],
        Gives::Array(vec![vec![2.0, 3.0, 4.0]]),
        LEGACY_POSITIONS,
        N(2.0)
    );
    // XLOOKUP over a lookup array of one cell: a return array of one row or column is returned
    // whole; one of more rows and columns is #VALUE!, before the search and after the modes.
    lk!(
        r,
        s,
        "XLOOKUP",
        [num("1"), rf(s, "A1"), rf(s, "B1:C2")],
        er(Value)
    );
    lk!(
        r,
        s,
        "XLOOKUP",
        [num("9"), rf(s, "A1"), rf(s, "B1:C2"), txt("nf")],
        er(Value)
    );
    lk!(
        r,
        s,
        "XLOOKUP",
        [num("9"), rf(s, "A1"), rf(s, "A13:C13")],
        er(NA)
    );
    lk_shape!(
        r,
        s,
        "XLOOKUP",
        [num("1"), rf(s, "A1"), rf(s, "A13:C13")],
        Gives::Area("A13:C13"),
        XLOOKUP_ONE_CELL,
        E(Value)
    );
    lk_shape!(
        r,
        s,
        "XLOOKUP",
        [num("1"), rf(s, "A1"), rf(s, "C1:C3")],
        Gives::Area("C1:C3"),
        XLOOKUP_ONE_CELL,
        E(Value)
    );
    lk!(
        r,
        s,
        "XLOOKUP",
        [num("1"), rf(s, "A1"), rf(s, "B1:C2"), omitted(), err(Div0)],
        er(Div0)
    );
    // A blank cell as XMATCH's search_mode is #VALUE!, as its match_mode exact.
    lk!(
        r,
        s,
        "XMATCH",
        [num("2"), asc(s), num("0"), cell(s, "C9")],
        er(Value)
    );
    lk!(r, s, "XMATCH", [num("2"), asc(s), cell(s, "C9")], n(2.0));
    // Blank or omitted arrays: #VALUE! in their argument's turn for INDEX, XMATCH, XLOOKUP and
    // LOOKUP; #N/A after every argument for MATCH, VLOOKUP and HLOOKUP.
    lk!(r, s, "INDEX", [omitted(), num("1")], er(Value));
    lk!(r, s, "INDEX", [omitted(), num("1"), err(Div0)], er(Value));
    lk!(r, s, "XMATCH", [num("1"), omitted()], er(Value));
    lk!(
        r,
        s,
        "XLOOKUP",
        [num("1"), omitted(), rf(s, "C1:C3")],
        er(Value)
    );
    lk!(
        r,
        s,
        "XLOOKUP",
        [num("1"), rf(s, "A1:A3"), omitted()],
        er(Value)
    );
    lk!(
        r,
        s,
        "XLOOKUP",
        [num("1"), rf(s, "A1:A3"), omitted(), omitted(), err(Div0)],
        er(Value)
    );
    lk!(r, s, "VLOOKUP", [num("1"), omitted(), num("1")], er(NA));
    lk!(
        r,
        s,
        "VLOOKUP",
        [num("1"), omitted(), num("1"), err(Div0)],
        er(Div0)
    );
    lk!(r, s, "LOOKUP", [num("1"), omitted()], er(Value));
    lk!(r, s, "LOOKUP", [num("1"), asc(s), omitted()], er(Value));
    lk!(r, s, "LOOKUP", [num("1"), err(Div0), omitted()], er(Div0));
    lk!(r, s, "MATCH", [num("1"), omitted(), err(Div0)], er(Div0));

    // ROW and COLUMN of several rows or columns inside a legacy formula: Excel writes `@` before
    // the inner call (`=SUM(@ROW(B3:B5))`), so the enclosing call receives the first number, as
    // CalcFml's compile gives it (dc-b1l); here the nested call's array, its first element taken.
    let b3_b5 = rf(s, "B3:B5");
    let b3_d3 = rf(s, "B3:D3");
    let row_at = at_first(nest!(s, "ROW", [b3_b5]));
    lk!(r, s, "SUM", [row_at.clone()], n(3.0));
    lk!(r, s, "MATCH", [num("4"), row_at, num("0")], er(NA));
    lk!(r, s, "SUM", [at_first(nest!(s, "COLUMN", [b3_d3]))], n(2.0));

    // Values that are not references, which only a nested call gives (Excel refuses ROW(5) at
    // entry): ROW, COLUMN and AREAS of them, and INDEX, ROWS and COLUMNS of a blank one, are
    // #VALUE!.
    lk!(
        r,
        s,
        "ROW",
        [nest!(s, "CHOOSE", [num("1"), num("5")])],
        er(Value)
    );
    lk!(
        r,
        s,
        "COLUMN",
        [nest!(s, "CHOOSE", [num("1"), txt("a")])],
        er(Value)
    );
    lk!(
        r,
        s,
        "AREAS",
        [nest!(s, "CHOOSE", [num("1"), num("5")])],
        er(Value)
    );
    for spelling in ["ROW", "AREAS", "ROWS", "COLUMNS"] {
        lk!(
            r,
            s,
            spelling,
            [nest!(s, "CHOOSE", [num("2"), num("1"), omitted()])],
            er(Value)
        );
    }
    lk!(
        r,
        s,
        "INDEX",
        [
            nest!(s, "CHOOSE", [num("2"), num("1"), omitted()]),
            num("1")
        ],
        er(Value)
    );

    // What a cell shows of the results above that it does not show as they are: XLOOKUP over a
    // lookup array of one cell gives the whole return array (its ROWS, COLUMNS and SUM), and a
    // blank result joins & as empty text.
    for (area, (height, width, sum)) in [("A13:C13", (1.0, 3.0, 6.0)), ("C1:C3", (3.0, 1.0, 60.0))]
    {
        let whole = nest!(s, "XLOOKUP", [num("1"), rf(s, "A1"), rf(s, area)]);
        lk!(r, s, "ROWS", [whole.clone()], n(height));
        lk!(r, s, "COLUMNS", [whole.clone()], n(width));
        lk!(r, s, "SUM", [whole], n(sum));
    }
    let blank_cell = nest!(
        s,
        "VLOOKUP",
        [num("3"), rf(s, "A9:C10"), num("3"), logical(false)]
    );
    lookup_row(
        r,
        s,
        AMPERSAND,
        &[blank_cell, txt("")],
        tx(""),
        Seen::Measured,
    );
    let omitted_value = nest!(s, "CHOOSE", [num("2"), txt("a"), omitted()]);
    lookup_row(
        r,
        s,
        AMPERSAND,
        &[omitted_value, txt("")],
        tx(""),
        Seen::Measured,
    );

    // The cases of the legacy tests (migration/ports/cfunc-m3.6.json names, for each test, the
    // rows that keep or change its expectation). MATCH:
    lk!(r, s, "MATCH", [num("3"), arr("{1,3}"), num("0")], n(2.0));
    lk!(r, s, "MATCH", [num("2.5"), arr("{1,2,3}")], n(2.0));
    lk!(
        r,
        s,
        "MATCH",
        [num("2"), arr("{1,2,2,2,3}"), num("1")],
        n(4.0)
    );
    lk!(r, s, "MATCH", [num("2"), arr("{1,2,2,2,3}")], n(4.0));
    lk!(
        r,
        s,
        "MATCH",
        [num("2"), arr("{1,2,2,2,3}"), omitted()],
        n(2.0)
    );
    lk!(
        r,
        s,
        "MATCH",
        [num("2"), arr("{3,2,2,2,1}"), num("-1")],
        n(2.0)
    );
    lk!(
        r,
        s,
        "MATCH",
        [num("2.5"), arr("{1,3,2,4}"), num("1")],
        n(1.0)
    );
    lk!(
        r,
        s,
        "MATCH",
        [num("2.5"), arr("{3,1,4,2}"), num("-1")],
        n(1.0)
    );
    lk!(
        r,
        s,
        "MATCH",
        [cell(s, "C9"), rf(s, "I5:J5"), num("0")],
        er(NA)
    );
    lk!(
        r,
        s,
        "MATCH",
        [num("9.99E+307"), rf(s, "H1:H4"), num("1")],
        n(4.0)
    );
    lk!(
        r,
        s,
        "MATCH",
        [num("2.5"), rf(s, "H6:H9"), num("1")],
        n(3.0)
    );
    lk!(
        r,
        s,
        "MATCH",
        [num("2.5"), rf(s, "H11:H14"), num("-1")],
        n(1.0)
    );
    lk!(
        r,
        s,
        "MATCH",
        [num("2.5"), arr("{1,#DIV/0!,2,3}"), num("1")],
        n(3.0)
    );
    lk!(
        r,
        s,
        "MATCH",
        [num("2.5"), arr("{1,#N/A,2,3}"), num("1")],
        n(3.0)
    );
    lk!(
        r,
        s,
        "MATCH",
        [txt("a~*"), arr("{\"abc\",\"a*\"}"), num("0")],
        n(2.0)
    );
    lk!(
        r,
        s,
        "MATCH",
        [txt("Abc"), arr("{\"abc\",\"def\"}"), num("0")],
        n(1.0)
    );
    lk!(r, s, "MATCH", [num("2"), arr("{1,2}"), num("0")], n(2.0));
    lk!(
        r,
        s,
        "MATCH",
        [num("2"), arr("{1,2;3,4}"), num("0")],
        er(NA)
    );
    lk!(
        r,
        s,
        "MATCH",
        [cell(s, "C17"), rf(s, "C16"), num("0")],
        er(NA)
    );
    lk!(
        r,
        s,
        "MATCH",
        [cell(s, "K1"), rf(s, "K2"), num("0")],
        er(NA)
    );
    lk_refused_shows!(
        r,
        s,
        LIFT_OWNER,
        "MATCH",
        [arr("{1,2,3}"), arr("{2,4,6,8}"), num("0")],
        refused(Some(0)),
        E(NA)
    );
    // XMATCH:
    lk!(r, s, "XMATCH", [num("2"), arr("{1,2,3}")], n(2.0));
    lk!(
        r,
        s,
        "XMATCH",
        [num("2"), arr("{2,1,2}"), num("0"), num("-1")],
        n(3.0)
    );
    lk!(r, s, "XMATCH", [num("9"), arr("{1,2}")], er(NA));
    lk!(
        r,
        s,
        "XMATCH",
        [cell(s, "C17"), rf(s, "C16"), num("0")],
        er(NA)
    );
    lk!(
        r,
        s,
        "XMATCH",
        [cell(s, "K1"), rf(s, "K2"), num("0")],
        er(NA)
    );
    lk!(
        r,
        s,
        "XMATCH",
        [txt("Abc"), arr("{\"abc\",\"zzz\"}")],
        n(1.0)
    );
    lk!(
        r,
        s,
        "XMATCH",
        [txt("\u{c4}bc"), arr("{\"\u{e4}BC\",\"zzz\"}")],
        n(1.0)
    );
    lk!(r, s, "XMATCH", [num("1"), arr("{\"1\"}")], er(NA));
    lk!(r, s, "XMATCH", [num("1"), arr("{1}"), num("9")], er(Value));
    lk!(
        r,
        s,
        "XMATCH",
        [num("1"), arr("{1}"), omitted(), num("9")],
        er(Value)
    );
    lk!(
        r,
        s,
        "XMATCH",
        [txt("a*"), arr("{\"zzz\",\"abc\"}"), num("2")],
        n(2.0)
    );
    lk!(
        r,
        s,
        "XMATCH",
        [txt("a~*"), arr("{\"abc\",\"a*\"}"), num("2")],
        n(2.0)
    );
    lk!(
        r,
        s,
        "XMATCH",
        [txt("a~?"), arr("{\"a1\",\"a?\"}"), num("2")],
        n(2.0)
    );
    lk!(
        r,
        s,
        "XMATCH",
        [txt("a~~b"), arr("{\"a~b\",\"ab\"}"), num("2")],
        n(1.0)
    );
    lk!(
        r,
        s,
        "XMATCH",
        [
            txt("a*"),
            arr("{\"abc\",\"def\",\"ade\"}"),
            num("2"),
            num("-1")
        ],
        n(3.0)
    );
    lk!(
        r,
        s,
        "XMATCH",
        [num("3"), arr("{1,2,3}"), omitted(), num("2")],
        n(3.0)
    );
    lk!(
        r,
        s,
        "XMATCH",
        [num("3"), arr("{4,3,2}"), omitted(), num("-2")],
        n(2.0)
    );
    lk!(
        r,
        s,
        "XMATCH",
        [num("2"), arr("{1,2,2,2,3}"), omitted(), num("2")],
        n(2.0)
    );
    lk!(
        r,
        s,
        "XMATCH",
        [num("2.9"), arr("{1,2,2,2,3}"), num("-1"), num("2")],
        n(4.0)
    );
    lk!(
        r,
        s,
        "XMATCH",
        [num("2"), arr("{3,2,2,2,1}"), omitted(), num("-2")],
        n(4.0)
    );
    lk!(
        r,
        s,
        "XMATCH",
        [num("2.5"), arr("{3,2,2,2,1}"), num("-1"), num("-2")],
        n(2.0)
    );
    lk!(
        r,
        s,
        "XMATCH",
        [num("2"), arr("{3,1,4,2}"), num("0"), num("2")],
        er(NA)
    );
    lk!(
        r,
        s,
        "XMATCH",
        [num("2.5"), arr("{3,1,4,2}"), num("1"), num("2")],
        n(3.0)
    );
    lk!(
        r,
        s,
        "XMATCH",
        [num("2.5"), arr("{3,1,4,2}"), num("-1"), num("2")],
        n(2.0)
    );
    lk!(
        r,
        s,
        "XMATCH",
        [num("9.99E+307"), rf(s, "H1:H4"), num("-1"), num("2")],
        n(2.0)
    );
    lk!(
        r,
        s,
        "XMATCH",
        [num("2.5"), rf(s, "H6:H9"), num("1"), num("2")],
        n(2.0)
    );
    lk!(
        r,
        s,
        "XMATCH",
        [num("2"), rf(s, "H6:H9"), omitted(), num("2")],
        er(NA)
    );
    lk!(
        r,
        s,
        "XMATCH",
        [num("2.5"), rf(s, "H11:H14"), num("-1"), num("-2")],
        n(3.0)
    );
    lk!(
        r,
        s,
        "XMATCH",
        [num("2.5"), arr("{1,#DIV/0!,2,3}"), num("1"), num("2")],
        n(2.0)
    );
    lk!(
        r,
        s,
        "XMATCH",
        [num("2.5"), arr("{1,2,3}"), num("1")],
        n(3.0)
    );
    lk!(
        r,
        s,
        "XMATCH",
        [num("2.5"), arr("{1,2,3}"), num("-1")],
        n(2.0)
    );
    lk!(r, s, "XMATCH", [num("2"), arr("{1,2,3}"), num("1")], n(2.0));
    lk!(
        r,
        s,
        "XMATCH",
        [num("2"), arr("{1,2,3}"), num("-1")],
        n(2.0)
    );
    lk!(
        r,
        s,
        "XMATCH",
        [
            txt("Beta"),
            arr("{\"alpha\",\"beta\",\"gamma\"}"),
            omitted(),
            num("2")
        ],
        n(2.0)
    );
    lk!(
        r,
        s,
        "XMATCH",
        [logical(true), arr("{FALSE,TRUE}"), omitted(), num("2")],
        n(2.0)
    );
    lk!(
        r,
        s,
        "XMATCH",
        [num("2"), arr("{1,2,3}"), omitted(), omitted()],
        n(2.0)
    );
    lk!(
        r,
        s,
        "XMATCH",
        [cell(s, "C9"), rf(s, "I1:I3"), num("0")],
        n(1.0)
    );
    lk!(
        r,
        s,
        "XMATCH",
        [omitted(), rf(s, "I1:I3"), num("0")],
        n(1.0)
    );
    lk!(r, s, "XMATCH", [txt(""), rf(s, "I1:I3"), num("0")], n(2.0));
    lk!(r, s, "XMATCH", [err(Value), arr("{1}")], er(Value));
    lk!(r, s, "XMATCH", [num("2"), arr("{#VALUE!}")], er(NA));
    lk!(r, s, "XMATCH", [num("2"), arr("{#DIV/0!,2}")], n(2.0));
    lk!(r, s, "XMATCH", [num("9"), arr("{1,#DIV/0!,2}")], er(NA));
    lk!(r, s, "XMATCH", [num("3"), arr("{3,4}")], n(1.0));
    // XLOOKUP:
    lk!(
        r,
        s,
        "XLOOKUP",
        [num("2"), arr("{1,2}"), arr("{\"one\",\"two\"}")],
        tx("two")
    );
    lk!(
        r,
        s,
        "XLOOKUP",
        [
            txt("a*"),
            arr("{\"abc\",\"ade\"}"),
            arr("{\"first\",\"last\"}"),
            omitted(),
            num("2"),
            num("-1")
        ],
        tx("last")
    );
    lk!(
        r,
        s,
        "XLOOKUP",
        [num("9"), arr("{1,2,3}"), arr("{10,20,30}"), txt("nf")],
        tx("nf")
    );
    lk!(
        r,
        s,
        "XLOOKUP",
        [
            num("2"),
            arr("{1,2,3}"),
            arr("{10,20,30}"),
            omitted(),
            num("0"),
            omitted()
        ],
        n(20.0)
    );
    lk!(
        r,
        s,
        "XLOOKUP",
        [omitted(), rf(s, "I5:J5"), arr("{10,20}")],
        n(10.0)
    );
    lk!(
        r,
        s,
        "XLOOKUP",
        [cell(s, "C9"), rf(s, "I5:J5"), arr("{10,20}")],
        n(10.0)
    );
    lk!(
        r,
        s,
        "XLOOKUP",
        [txt(""), rf(s, "I7:J7"), arr("{10,20}")],
        n(10.0)
    );
    lk!(
        r,
        s,
        "XLOOKUP",
        [num("1"), arr("{1,2}"), rf(s, "I9:J9")],
        at_n("I9", 0.0)
    );
    lk!(
        r,
        s,
        "XLOOKUP",
        [
            num("2"),
            arr("{3,1,4,2}"),
            arr("{30,10,40,20}"),
            omitted(),
            num("0"),
            num("2")
        ],
        er(NA)
    );
    lk!(
        r,
        s,
        "XLOOKUP",
        [num("2"), arr("{1,2,3}"), rf(s, "B1:D1")],
        at_n("C1", 10.0)
    );
    lk!(
        r,
        s,
        "XLOOKUP",
        [num("2"), arr("{1;2;3}"), arr("{10,20,30}")],
        er(Value)
    );
    lk_shape!(
        r,
        s,
        "XLOOKUP",
        [num("2"), arr("{1;2;3}"), arr("{10,11;20,21;30,31}")],
        Gives::Array(vec![vec![20.0, 21.0]]),
        XLOOKUP_PART,
        N(20.0)
    );
    lk_shape!(
        r,
        s,
        "XLOOKUP",
        [num("2"), arr("{1,2,3}"), rf(s, "B2:D4")],
        Gives::Area("C2:C4"),
        XLOOKUP_PART,
        E(Value)
    );
    lk_refused_shows!(
        r,
        s,
        LIFT_OWNER,
        "XLOOKUP",
        [arr("{1,2,3}"), arr("{2,4,6,8}"), arr("{20,40,60,80}")],
        refused(Some(0)),
        E(NA)
    );
    lk_refused_shows!(
        r,
        s,
        LIFT_OWNER,
        "XLOOKUP",
        [
            arr("{1,2;3,4}"),
            arr("{2,4,6,8}"),
            arr("{20,40,60,80}"),
            arr("{\"NF\",\"ignored\"}")
        ],
        refused(Some(0)),
        t("NF")
    );
    lk_refused_shows!(
        r,
        s,
        LIFT_OWNER,
        "XLOOKUP",
        [arr("{2,4}"), arr("{2;4;6}"), arr("{20,21;40,41;60,61}")],
        refused(Some(0)),
        N(20.0)
    );
    lk_refused_shows!(
        r,
        s,
        LIFT_OWNER,
        "XLOOKUP",
        [arr("{2,4}"), arr("{2,4,6}"), arr("{20,40,60;21,41,61}")],
        refused(Some(0)),
        N(20.0)
    );
    // VLOOKUP and HLOOKUP over the tables {1,10;2,20;3,30} and {1,2,3;10,20,30}:
    let tall = || arr("{1,10;2,20;3,30}");
    let flat = || arr("{1,2,3;10,20,30}");
    lk!(
        r,
        s,
        "VLOOKUP",
        [num("2"), tall(), num("2"), logical(false)],
        n(20.0)
    );
    lk!(r, s, "VLOOKUP", [num("2.9"), tall(), num("2")], n(20.0));
    lk!(
        r,
        s,
        "HLOOKUP",
        [num("2"), flat(), num("2"), logical(false)],
        n(20.0)
    );
    lk!(r, s, "HLOOKUP", [num("2.9"), flat(), num("2")], n(20.0));
    lk!(
        r,
        s,
        "VLOOKUP",
        [num("2.9"), rf(s, "L1:M4"), num("2")],
        n(20.0)
    );
    lk!(
        r,
        s,
        "VLOOKUP",
        [num("9.99E+307"), rf(s, "L1:M4"), num("2")],
        n(30.0)
    );
    lk!(
        r,
        s,
        "HLOOKUP",
        [num("2.9"), rf(s, "L6:O7"), num("2")],
        n(20.0)
    );
    lk!(
        r,
        s,
        "HLOOKUP",
        [num("9.99E+307"), rf(s, "L6:O7"), num("2")],
        n(30.0)
    );
    lk!(r, s, "VLOOKUP", [num("0.5"), tall(), num("2")], er(NA));
    lk!(r, s, "HLOOKUP", [num("0.5"), flat(), num("2")], er(NA));
    lk!(
        r,
        s,
        "VLOOKUP",
        [num("4"), tall(), num("2"), logical(false)],
        er(NA)
    );
    lk!(
        r,
        s,
        "HLOOKUP",
        [num("4"), flat(), num("2"), logical(false)],
        er(NA)
    );
    lk!(
        r,
        s,
        "VLOOKUP",
        [num("2"), tall(), num("1.9"), logical(false)],
        n(2.0)
    );
    lk!(
        r,
        s,
        "HLOOKUP",
        [num("2"), flat(), num("1.9"), logical(false)],
        n(2.0)
    );
    lk!(
        r,
        s,
        "VLOOKUP",
        [num("2"), tall(), num("3"), logical(false)],
        er(Ref)
    );
    lk!(
        r,
        s,
        "HLOOKUP",
        [num("2"), flat(), num("3"), logical(false)],
        er(Ref)
    );
    for index in ["0", "-1"] {
        lk!(
            r,
            s,
            "VLOOKUP",
            [num("2"), tall(), num(index), logical(false)],
            er(Value)
        );
        lk!(
            r,
            s,
            "HLOOKUP",
            [num("2"), flat(), num(index), logical(false)],
            er(Value)
        );
    }
    lk!(
        r,
        s,
        "VLOOKUP",
        [
            txt("b*"),
            arr("{\"abc\",10;\"bcd\",20}"),
            num("2"),
            logical(false)
        ],
        n(20.0)
    );
    lk!(
        r,
        s,
        "HLOOKUP",
        [
            txt("b*"),
            arr("{\"abc\",\"bcd\";10,20}"),
            num("2"),
            logical(false)
        ],
        n(20.0)
    );
    lk!(
        r,
        s,
        "VLOOKUP",
        [
            logical(true),
            arr("{TRUE,1;FALSE,2}"),
            num("2"),
            logical(false)
        ],
        n(1.0)
    );
    lk_refused_shows!(
        r,
        s,
        LIFT_OWNER,
        "VLOOKUP",
        [
            arr("{1,2,3}"),
            arr("{2,20;4,40;6,60;8,80}"),
            num("2"),
            logical(false)
        ],
        refused(Some(0)),
        E(NA)
    );
    lk_refused_shows!(
        r,
        s,
        LIFT_OWNER,
        "HLOOKUP",
        [
            arr("{1,2,3}"),
            arr("{2,4,6,8;20,40,60,80}"),
            num("2"),
            logical(false)
        ],
        refused(Some(0)),
        E(NA)
    );
    // LOOKUP:
    lk!(
        r,
        s,
        "LOOKUP",
        [num("2.9"), arr("{1,2,3}"), arr("{10,20,30}")],
        n(20.0)
    );
    lk!(r, s, "LOOKUP", [num("2.9"), tall()], n(20.0));
    lk!(
        r,
        s,
        "LOOKUP",
        [num("2.2"), arr("{1,2,3}"), arr("{\"a\",\"b\",\"c\"}")],
        tx("b")
    );
    lk!(
        r,
        s,
        "LOOKUP",
        [num("2"), arr("{2,1,3}"), arr("{20,10,30}")],
        n(10.0)
    );
    lk!(
        r,
        s,
        "LOOKUP",
        [num("2"), arr("{1,2;3,4;5,6}"), arr("{10,20,30}")],
        n(10.0)
    );
    // CHOOSE:
    lk!(
        r,
        s,
        "CHOOSE",
        [num("2.9"), rf(s, "A1"), txt("picked"), err(Div0)],
        tx("picked")
    );
    lk!(
        r,
        s,
        "CHOOSE",
        [num("0.9"), num("10"), num("20")],
        er(Value)
    );
    lk!(
        r,
        s,
        "CHOOSE",
        [num("4"), num("10"), num("20"), num("30")],
        er(Value)
    );
    lk!(
        r,
        s,
        "CHOOSE",
        [num("1"), rf(s, "C9"), num("7")],
        at_n("C9", 0.0)
    );
    lk_refused_shows!(
        r,
        s,
        LIFT_OWNER,
        "CHOOSE",
        [
            arr("{1,2,3,4}"),
            txt("Alpha"),
            txt("Bravo"),
            txt("Charlie"),
            txt("Delta")
        ],
        refused(Some(0)),
        t("Alpha")
    );
    lk_refused_shows!(
        r,
        s,
        LIFT_OWNER,
        "CHOOSE",
        [
            arr("{1,2,3}"),
            arr("{1;2;3}"),
            arr("{10;20;30}"),
            arr("{100;200;300}")
        ],
        refused(Some(0)),
        N(1.0)
    );
    // The structured-reference tests' cases, over an array and an area of the sheet:
    lk!(
        r,
        s,
        "MATCH",
        [
            txt("South"),
            arr("{\"North\",\"South\",\"North\"}"),
            num("0")
        ],
        n(2.0)
    );
    lk!(
        r,
        s,
        "XLOOKUP",
        [
            txt("South"),
            arr("{\"North\",\"South\",\"North\"}"),
            arr("{100,200,300}")
        ],
        n(200.0)
    );
    lk!(r, s, "ROWS", [rf(s, "A1:C7")], n(7.0));
    lk!(r, s, "COLUMNS", [rf(s, "A1:C7")], n(3.0));
    lk!(
        r,
        s,
        "INDEX",
        [rf(s, "A1:C7"), num("2"), num("3")],
        at_n("C2", 20.0)
    );
    lk!(r, s, "INDEX", [rf(s, "A1:C7"), num("8"), num("1")], er(Ref));
    lk!(r, s, "AREAS", [rf(s, "A1:C7")], n(1.0));

    // XMATCH's order of the errors a cell constant can hold: XMATCH(TRUE,pair,1), the next larger
    // value than TRUE, is the position of the smaller error of the pair.
    let place = |code: WorksheetErrorCode| {
        ERROR_LITERALS
            .iter()
            .position(|(known, _)| *known == code)
            .expect("an error the fixtures write")
    };
    for (row, (first, second)) in error_pairs().into_iter().enumerate() {
        let pair = rf(s, &format!("F{0}:G{0}", row + 1));
        let smaller = if place(first) < place(second) {
            1.0
        } else {
            2.0
        };
        lk!(r, s, "XMATCH", [logical(true), pair, num("1")], n(smaller));
    }
    // The same over the pairs with #SPILL!, #FIELD! or #CALC! among them, whose source formulas
    // the reader holds as the errors Excel gives them: each row is conditional on those errors.
    let ordered = |code: WorksheetErrorCode| {
        ORDERED_ERRORS
            .iter()
            .position(|(known, _)| *known == code)
            .expect("an error XMATCH orders")
    };
    for (row, (first, second)) in source_error_pairs().into_iter().enumerate() {
        let pair = rf(s, &format!("P{0}:Q{0}", row + 1));
        let smaller = if ordered(first) < ordered(second) {
            1.0
        } else {
            2.0
        };
        lk!(r, s, "XMATCH", [logical(true), pair, num("1")], n(smaller));
    }

    // A linear next-smaller or next-larger search decides by kind before it orders within one:
    // texts calc_func's comparison cannot order are passed over when a nearer kind holds the
    // result.
    lk!(
        r,
        s,
        "XMATCH",
        [num("5"), arr("{\"a b\",\"c d\",6}"), num("1")],
        n(3.0)
    );
    lk!(
        r,
        s,
        "XMATCH",
        [logical(true), arr("{\"a b\",\"c d\",FALSE}"), num("-1")],
        n(3.0)
    );
    lk!(
        r,
        s,
        "XLOOKUP",
        [
            num("5"),
            arr("{\"a b\",\"c d\",6}"),
            arr("{10,20,30}"),
            omitted(),
            num("1")
        ],
        n(30.0)
    );

    // The rules of the per-ID manifest that earlier rounds read from Excel ad hoc only.
    let halves = |s: &TestSheet| reference(s, &["A1:A3", "A4:A6"]);
    let long = || txt(&"a".repeat(256));
    // MATCH: match_type 0.5 is 1 and a blank cell 0; several areas, an omitted lookup array and
    // a single value given directly are #N/A; a 1-by-1 array and a one-cell reference are
    // searched.
    lk!(r, s, "MATCH", [num("2"), asc(s), num("0.5")], n(4.0));
    lk!(r, s, "MATCH", [num("2"), asc(s), cell(s, "C9")], n(2.0));
    lk!(r, s, "MATCH", [num("2"), halves(s), num("0")], er(NA));
    lk!(r, s, "MATCH", [num("1"), omitted(), num("0")], er(NA));
    lk!(r, s, "MATCH", [num("2"), arr("{2}"), num("0")], n(1.0));
    lk!(r, s, "MATCH", [num("1"), rf(s, "A1"), num("0")], n(1.0));
    // XMATCH: modes rounded down; several areas #N/A; no limit on the text; a blank lookup array
    // #VALUE! before the modes, after the lookup value's error.
    lk!(r, s, "XMATCH", [num("2.5"), asc(s), num("1.5")], n(5.0));
    lk!(
        r,
        s,
        "XMATCH",
        [num("3"), desc(s), num("0"), num("-1.5")],
        n(4.0)
    );
    lk!(r, s, "XMATCH", [num("2"), halves(s)], er(NA));
    lk!(r, s, "XMATCH", [long(), fruit(s)], er(NA));
    lk!(r, s, "XMATCH", [num("1"), omitted(), err(Div0)], er(Value));
    lk!(r, s, "XMATCH", [err(NA), omitted()], er(NA));
    // XLOOKUP: a lookup array of more rows and columns #VALUE!; a lookup or return reference of
    // several areas #N/A.
    lk!(
        r,
        s,
        "XLOOKUP",
        [num("2"), rf(s, "A1:B6"), tens(s)],
        er(Value)
    );
    lk!(r, s, "XLOOKUP", [num("2"), halves(s), tens(s)], er(NA));
    lk!(
        r,
        s,
        "XLOOKUP",
        [num("2"), asc(s), reference(s, &["C1:C3", "C4:C6"])],
        er(NA)
    );
    // VLOOKUP and HLOOKUP: range_lookup 0 and a blank cell exact, a nonzero number approximate;
    // the first matching row; argument errors in order; several areas, a single value and a blank
    // table #N/A; text over 255 code units #VALUE!.
    lk!(
        r,
        s,
        "VLOOKUP",
        [num("2.5"), table(s), num("2"), num("0")],
        er(NA)
    );
    lk!(
        r,
        s,
        "VLOOKUP",
        [num("2.5"), table(s), num("2"), cell(s, "C9")],
        er(NA)
    );
    lk!(
        r,
        s,
        "VLOOKUP",
        [num("2"), table(s), num("3"), logical(false)],
        n(20.0)
    );
    lk!(
        r,
        s,
        "VLOOKUP",
        [num("2.5"), table(s), num("3"), num("5")],
        n(40.0)
    );
    lk!(r, s, "VLOOKUP", [err(NA), table(s), err(Div0)], er(NA));
    lk!(
        r,
        s,
        "VLOOKUP",
        [
            num("2"),
            reference(s, &["A1:B3", "A4:B6"]),
            num("2"),
            logical(false)
        ],
        er(NA)
    );
    lk!(r, s, "VLOOKUP", [num("2"), num("2"), num("1")], er(NA));
    lk!(
        r,
        s,
        "VLOOKUP",
        [long(), rf(s, "B1:C6"), num("2"), logical(false)],
        er(Value)
    );
    lk!(r, s, "HLOOKUP", [num("1"), omitted(), num("1")], er(NA));
    // LOOKUP: a grid wider than tall searched in its first row and answered from its last; a
    // single result cell; a result of more rows and columns, or shorter than the position, #N/A;
    // several areas #N/A; text over 255 code units #VALUE!; a blank result cell; a blank result
    // vector #VALUE! though nothing matches.
    lk!(r, s, "LOOKUP", [num("2"), rf(s, "A13:C14")], tx("q"));
    lk!(r, s, "LOOKUP", [num("1"), asc(s), rf(s, "C1")], n(10.0));
    lk!(
        r,
        s,
        "LOOKUP",
        [num("2"), rf(s, "A1:A3"), rf(s, "A13:C15")],
        er(NA)
    );
    lk!(r, s, "LOOKUP", [num("3"), arr("{1,2,3}"), num("5")], er(NA));
    lk!(r, s, "LOOKUP", [num("2"), halves(s), tens(s)], er(NA));
    lk!(
        r,
        s,
        "LOOKUP",
        [num("2"), asc(s), reference(s, &["C1:C3", "C4:C6"])],
        er(NA)
    );
    lk!(r, s, "LOOKUP", [long(), fruit(s)], er(Value));
    lk_shape!(
        r,
        s,
        "LOOKUP",
        [num("1"), arr("{1,2}"), rf(s, "I9:J9")],
        Gives::Blank,
        BLANK_RESULT,
        N(0.0)
    );
    lk!(r, s, "LOOKUP", [num("0"), asc(s), omitted()], er(Value));
    // CHOOSE: FALSE and a blank cell as index_num are 0.
    lk!(
        r,
        s,
        "CHOOSE",
        [logical(false), txt("a"), txt("b")],
        er(Value)
    );
    lk!(
        r,
        s,
        "CHOOSE",
        [cell(s, "C9"), txt("a"), txt("b")],
        er(Value)
    );
    // COLUMNS: an array, a single value, an error and several areas.
    lk!(r, s, "COLUMNS", [arr("{1,2,3}")], n(3.0));
    lk!(r, s, "COLUMNS", [num("5")], n(1.0));
    lk!(r, s, "COLUMNS", [err(NA)], er(NA));
    lk!(
        r,
        s,
        "COLUMNS",
        [reference(s, &["A1:A3", "B1:B2"])],
        er(Ref)
    );
    rows
}

#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test)]
fn cfunc_m3_6__lookup_edges() {
    const CASE: &str = "cfunc_m3_6__lookup_edges";
    let fast = NumericMode::Fast;
    let mut sheet = lookup_edges_sheet();
    note!(
        "ACCEPT-NOTE: {CASE}: the sheet holds {}; every other cell is blank",
        sheet.described()
    );
    let rows = lookup_edges_rows(&mut sheet, false);
    let s = &mut sheet;

    // Metadata and dispatch come from the one generated catalogue: family 9 holds exactly these
    // entries; CHOOSE alone is lazy; ROW and COLUMN take an optional reference.
    let family: BTreeSet<(u32, String)> = catalogue()
        .targets()
        .filter(|target| target.id().raw() >> 16 == 9)
        .map(|target| (target.id().raw(), target.spelling().to_owned()))
        .collect();
    let expected: BTreeSet<(u32, String)> = LOOKUPS
        .iter()
        .map(|(spelling, member)| ((9 << 16) | member, (*spelling).to_owned()))
        .collect();
    assert_eq!(family, expected, "the lookup family in the catalogue");
    let shapes: [(&str, (usize, usize)); 13] = [
        ("INDEX", (2, 4)),
        ("MATCH", (2, 3)),
        ("XMATCH", (2, 4)),
        ("XLOOKUP", (3, 6)),
        ("VLOOKUP", (3, 4)),
        ("HLOOKUP", (3, 4)),
        ("LOOKUP", (2, 3)),
        ("CHOOSE", (2, 255)),
        ("ROW", (0, 1)),
        ("COLUMN", (0, 1)),
        ("ROWS", (1, 1)),
        ("COLUMNS", (1, 1)),
        ("AREAS", (1, 1)),
    ];
    for (spelling, (min, max)) in shapes {
        let target = surface(Notation::Call, spelling);
        assert_eq!(
            (target.arity().min, target.arity().max),
            (min, max),
            "{spelling}"
        );
        let demand = if spelling == "CHOOSE" {
            ArgumentDemand::BranchOnCondition
        } else {
            ArgumentDemand::Eager
        };
        assert_eq!(target.argument_demand(), demand, "{spelling}");
    }
    let classes: [(&str, usize, ParameterClass); 10] = [
        ("INDEX", 0, ParameterClass::ValueOrReference),
        ("INDEX", 1, ParameterClass::Value),
        ("MATCH", 0, ParameterClass::Value),
        ("MATCH", 1, ParameterClass::ValueOrReference),
        ("VLOOKUP", 1, ParameterClass::ValueOrReference),
        ("HLOOKUP", 1, ParameterClass::ValueOrReference),
        ("XLOOKUP", 2, ParameterClass::ValueOrReference),
        ("XLOOKUP", 3, ParameterClass::ValueOrReference),
        ("CHOOSE", 254, ParameterClass::ValueOrReference),
        ("AREAS", 0, ParameterClass::ValueOrReference),
    ];
    for (spelling, index, class) in classes {
        assert_eq!(
            surface(Notation::Call, spelling).parameter_class(index),
            Some(class),
            "{spelling} argument {index}"
        );
    }
    let helps = [
        ("INDEX", "INDEX(array, row_num, [column_num], [area_num])"),
        ("MATCH", "MATCH(lookup_value, lookup_array, [match_type])"),
        (
            "XLOOKUP",
            "XLOOKUP(lookup_value, lookup_array, return_array, [if_not_found], [match_mode], [search_mode])",
        ),
        (
            "VLOOKUP",
            "VLOOKUP(lookup_value, table_array, col_index_num, [range_lookup])",
        ),
        ("CHOOSE", "CHOOSE(index_num, value1, [value2], ...)"),
        ("ROW", "ROW([reference])"),
    ];
    for (spelling, display) in helps {
        assert_eq!(
            surface(Notation::Call, spelling).signature_help().display,
            display
        );
    }
    let manifest = manifest_ids(LOOKUP_RECORD);
    let listed: BTreeSet<(String, String)> = LOOKUPS
        .iter()
        .map(|(spelling, member)| (format!("9.{member}"), (*spelling).to_owned()))
        .collect();
    assert_eq!(
        manifest, listed,
        "the per-ID manifest of migration/ports/cfunc-m3.6.json against the catalogue"
    );
    note!(
        "ACCEPT-NOTE: {CASE}: the catalogue's family 9 holds exactly the {} entries {:?}; CHOOSE alone is lazy (BranchOnCondition) and ROW and COLUMN take 0 or 1 argument; the per-ID manifest of migration/ports/cfunc-m3.6.json lists the same IDs and spellings",
        family.len(),
        LOOKUPS.map(|(spelling, _)| spelling)
    );

    // CHOOSE asks for index_num and then the chosen value only: the values it does not choose,
    // an error among them, are never evaluated.
    let choose = surface(Notation::Call, "CHOOSE");
    let mut held_back = Recorded::new(vec![
        CalcValue::number(3.0),
        CalcValue::error(WorksheetErrorCode::Div0),
        CalcValue::error(WorksheetErrorCode::NA),
        text_value("c"),
    ]);
    assert_eq!(run(choose, &mut held_back, &fast), Ok(text_value("c")));
    assert_eq!(held_back.asked, [0, 3]);
    let mut out_of_range = Recorded::new(vec![CalcValue::number(5.0), text_value("a")]);
    assert_eq!(
        run(choose, &mut out_of_range, &fast),
        Ok(CalcValue::error(WorksheetErrorCode::Value))
    );
    assert_eq!(out_of_range.asked, [0]);
    note!(
        "ACCEPT-NOTE: {CASE}: CHOOSE(3,#DIV/0!,#N/A,\"c\") asked for arguments {:?} and gave \"c\"; CHOOSE(5,\"a\") asked for {:?}",
        held_back.asked,
        out_of_range.asked
    );

    // The measured Giyeok-A leading primary follows every ASCII entry in B1:B6.
    // Approximate XMATCH therefore selects its final entry through the ordinary lookup route.
    let xmatch = surface(Notation::Call, "XMATCH");
    assert_eq!(invoke(
        xmatch,
        &mut Recorded::new(vec![
            text_value("\u{1100}\u{1161}"),
            reference(s, &["B1:B6"]).value,
            CalcValue::number(-1.0),
        ]),
        &mut EvalServices::with_cells(&fast, s),
        &mut FunctionCallScratch::new(),
    ), Ok(CalcValue::number(6.0)));

    // Through the call interface alone (no row of the workbook): the formula's own cell, which
    // ROW() and COLUMN() need, is not among the services (dc-m3u); regex matching (match_mode 3,
    // dc-uar), an array index_num (dc-m3u) and a read past LOOKUP's result reference
    // (dc-m3u) are refused; so is a text comparison calc_func's comparison cannot decide (text
    // holding Hangul conjoining jamo, which compose with each other: dc-zsn.1's collation builds no
    // sort key for them), and an order between #BUSY!, #BLOCKED! or #CONNECT! and another error,
    // which no formula on this machine gives offline (dc-uar).
    let refusals: [(&str, Vec<CalcValue>, Option<usize>); 7] = [
        ("ROW", vec![], None),
        ("COLUMN", vec![], None),
        (
            "XMATCH",
            vec![
                CalcValue::number(2.0),
                reference(s, &["A1:A6"]).value,
                CalcValue::number(3.0),
            ],
            Some(2),
        ),
        (
            "CHOOSE",
            vec![
                CalcValue::array(CalcArray::from_scalar(CalcValue::number(1.0))),
                text_value("a"),
            ],
            Some(0),
        ),
        (
            "LOOKUP",
            vec![
                CalcValue::number(3.0),
                reference(s, &["A1:A6"]).value,
                reference(s, &["B1"]).value,
            ],
            Some(2),
        ),
        (
            "XMATCH",
            vec![
                text_value("\u{1103}\u{1161}"),
                reference(s, &["B1:B6"]).value,
                CalcValue::number(-1.0),
            ],
            None,
        ),
        (
            "XMATCH",
            vec![
                CalcValue::logical(true),
                CalcValue::array(
                    CalcArray::from_rows(vec![vec![
                        CalcValue::error(WorksheetErrorCode::Busy),
                        CalcValue::error(WorksheetErrorCode::NA),
                    ]])
                    .expect("a row"),
                ),
                CalcValue::number(1.0),
            ],
            None,
        ),
    ];
    for (spelling, values, argument) in refusals {
        let target = surface(Notation::Call, spelling);
        let result = invoke(
            target,
            &mut Recorded::new(values),
            &mut EvalServices::with_cells(&fast, s),
            &mut FunctionCallScratch::new(),
        );
        assert_eq!(
            result,
            Err(FunctionFault::NotImplemented {
                id: target.id(),
                argument
            }),
            "{spelling}"
        );
    }
    note!(
        "ACCEPT-NOTE: {CASE}: refused with NotImplemented: ROW() and COLUMN() (the formula's own cell is not supplied; owned by {}: {}), XMATCH(2,A1:A6,3) (regex; dc-uar), CHOOSE({{1}},\"a\") (lifted; dc-m3u), LOOKUP(3,A1:A6,B1) (Excel reads past B1; owned by {}: {}), XMATCH(\"\u{1103}\u{1161}\",B1:B6,-1) (unmeasured leading Hangul conjoining jamo; dc-zsn.2), XMATCH(TRUE,{{#BUSY!,#N/A}},1) (an order of #BUSY! no formula on this machine gives offline; dc-uar)",
        CALLER_OWNER.bead,
        CALLER_OWNER.behaviour,
        READ_PAST_OWNER.bead,
        READ_PAST_OWNER.behaviour
    );

    // The errors that neither a cell constant nor a formula literal can hold (Excel refuses to
    // open a workbook with #SPILL!, #FIELD! or #CALC! as a cell constant, and refuses each as a
    // formula literal) come from source formulas in P and Q: FILTER, SEQUENCE and FIELDVALUE.
    // calc_func's catalogue has FILTER and SEQUENCE since F3 (dc-728) and FIELDVALUE since dc-b1l,
    // but FILTER's and SEQUENCE's errors here rest on the engine (SEQUENCE's spill blocked by R:
    // #SPILL!), so this case evaluates none of the three source formulas. The reader holds for
    // each the error Excel gives it, so XMATCH(TRUE,P{k}:Q{k},1) over the 54 ordered pairs with one
    // of them among the two errors is a row conditional on those errors, which the comparison
    // checks against the capture, with each source cell's error. FIELDVALUE's own call gives the
    // #FIELD! the reader holds.
    let field = invoke(
        surface(Notation::Call, "FIELDVALUE"),
        &mut Recorded::new(vec![
            CalcValue::number(1.0),
            CalcValue::text(ExcelText::from_interop_assignment("x")),
        ]),
        &mut EvalServices::new(&fast),
        &mut FunctionCallScratch::new(),
    );
    assert_eq!(
        field,
        Ok(CalcValue::error(WorksheetErrorCode::Field)),
        "FIELDVALUE(1,\"x\")"
    );
    let sources = lookup_sources("lookup_edges");
    let conditional: Vec<u32> = rows
        .oracle
        .iter()
        .filter(|row| row.owner == Some(SOURCE_READ))
        .map(|row| row.row)
        .collect();
    assert_eq!(
        conditional.len(),
        source_error_pairs().len(),
        "one conditional row per pair"
    );
    note!(
        "ACCEPT-NOTE: {CASE}: FIELDVALUE(1,\"x\") is #FIELD!; the {} source cells of P1:Q{} hold =FILTER(A1:A6,A1:A6>100) (#CALC!), =SEQUENCE(1,2) blocked by the cell to its right (#SPILL!) and =FIELDVALUE(A1,\"x\") (#FIELD!) as dynamic-array formulas, for which the reader holds those errors; XMATCH(TRUE,P{{k}}:Q{{k}},1) over the {} ordered pairs of two errors with #SPILL!, #FIELD! or #CALC! among them, rows D{} to D{}, picks the smaller error in the order #NULL!, #DIV/0!, #VALUE!, #REF!, #NAME?, #NUM!, #N/A, #GETTING_DATA, #SPILL!, #FIELD!, #CALC!; each is conditional on its source cells' errors, which the comparison checks against the capture",
        sources.len(),
        source_error_pairs().len(),
        conditional.len(),
        conditional.first().expect("a conditional row"),
        conditional.last().expect("a conditional row")
    );

    // -0 orders as +0, as matching compares it: MATCH's descending walk meets -0 as equal to a 0
    // lookup value, and XMATCH's next smaller value keeps the first of -0 and 0 as tied. A
    // worksheet cell holds no -0, so this is the kernel's own consistency, not a row Excel shows.
    let mut signed = TestSheet::new(&[
        (
            "A1",
            "-0 (no worksheet cell holds it)",
            CalcValue::number(-0.0),
        ),
        ("A2", "0", CalcValue::number(0.0)),
    ]);
    for (spelling, values, position) in [
        (
            "MATCH",
            vec![
                CalcValue::number(0.0),
                reference(&signed, &["A1"]).value,
                CalcValue::number(-1.0),
            ],
            1.0,
        ),
        (
            "XMATCH",
            vec![
                CalcValue::number(1.0),
                reference(&signed, &["A1:A2"]).value,
                CalcValue::number(-1.0),
            ],
            1.0,
        ),
    ] {
        let found = invoke(
            surface(Notation::Call, spelling),
            &mut Recorded::new(values),
            &mut EvalServices::with_cells(&fast, &mut signed),
            &mut FunctionCallScratch::new(),
        );
        assert_eq!(found, Ok(CalcValue::number(position)), "{spelling} over -0");
    }
    note!(
        "ACCEPT-NOTE: {CASE}: over a reader holding -0 in A1 and 0 in A2, MATCH(0,A1,-1) is 1 and XMATCH(1,A1:A2,-1) is 1: -0 orders as +0, as matching compares it"
    );

    // ROW and COLUMN of an area of several rows or columns: the array of every number (dc-b1l),
    // up to a whole row's 16,384 columns; a whole column's rows are refused, not built.
    let positions = |spelling: &str, area: &str| {
        let value = reference(s, &[area]).value;
        run(
            surface(Notation::Call, spelling),
            &mut Recorded::new(vec![value]),
            &fast,
        )
    };
    let numbers = |rows: Vec<Vec<f64>>| {
        Ok(CalcValue::array(
            CalcArray::from_rows(
                rows.into_iter()
                    .map(|row| row.into_iter().map(CalcValue::number).collect())
                    .collect(),
            )
            .expect("an array"),
        ))
    };
    assert_eq!(
        positions("ROW", "B2:C3"),
        numbers(vec![vec![2.0], vec![3.0]])
    );
    assert_eq!(positions("COLUMN", "B2:C3"), numbers(vec![vec![2.0, 3.0]]));
    assert_eq!(
        positions("COLUMN", "A1:XFD1"),
        numbers(vec![(1..=16_384).map(f64::from).collect()])
    );
    let row_id = surface(Notation::Call, "ROW").id();
    assert_eq!(
        positions("ROW", "A:A"),
        Err(FunctionFault::NotImplemented {
            id: row_id,
            argument: Some(0)
        })
    );
    note!(
        "ACCEPT-NOTE: {CASE}: ROW(B2:C3) is {{2;3}}, COLUMN(B2:C3) {{2,3}} and COLUMN(A1:XFD1) the 16,384 column numbers; ROW(A:A) is refused with NotImplemented naming argument 0 (its array is not built); the oracle rows ROW(B3:B5), COLUMN(B3:D3), SUM(ROW(B3:B5)), MATCH(4,ROW(B3:B5),0) and SUM(COLUMN(B3:D3)) state what Excel shows in a legacy formula, which the capture judges"
    );

    // A reference with no cell reader, or one the reader refuses or breaks the contract on,
    // ends the call in ReferenceUnreadable naming the argument.
    let lookups: [(&str, Vec<CalcValue>, usize); 5] = [
        // The exact searches stop at a match, so their lookup values match nothing and the walk
        // meets every cell the reader visits; the approximate ones read the whole line.
        (
            "MATCH",
            vec![
                CalcValue::number(99.0),
                reference(s, &["A1:A6"]).value,
                CalcValue::number(0.0),
            ],
            1,
        ),
        (
            "XMATCH",
            vec![CalcValue::number(99.0), reference(s, &["A1:A6"]).value],
            1,
        ),
        (
            "VLOOKUP",
            vec![
                CalcValue::number(2.0),
                reference(s, &["A1:C6"]).value,
                CalcValue::number(3.0),
            ],
            1,
        ),
        (
            "LOOKUP",
            vec![
                CalcValue::number(2.0),
                reference(s, &["A1:A6"]).value,
                reference(s, &["C1:C6"]).value,
            ],
            1,
        ),
        (
            "XLOOKUP",
            vec![
                CalcValue::number(99.0),
                reference(s, &["A1:A6"]).value,
                reference(s, &["C1:C6"]).value,
            ],
            1,
        ),
    ];
    let mut unreadable = 0;
    for (spelling, values, index) in &lookups {
        let target = surface(Notation::Call, spelling);
        let fault = Err(FunctionFault::ReferenceUnreadable {
            id: target.id(),
            index: *index,
        });
        assert_eq!(
            run(target, &mut Recorded::new(values.clone()), &fast),
            fault,
            "{spelling} without a reader"
        );
        s.refuse = true;
        let refused_read = invoke(
            target,
            &mut Recorded::new(values.clone()),
            &mut EvalServices::with_cells(&fast, s),
            &mut FunctionCallScratch::new(),
        );
        s.refuse = false;
        assert_eq!(refused_read, fault, "{spelling} over a refusing reader");
        for breach in [Breach::Outside("C40"), Breach::Reversed] {
            let mut breaching = BreachingReader { sheet: s, breach };
            let broken = invoke(
                target,
                &mut Recorded::new(values.clone()),
                &mut EvalServices::with_cells(&fast, &mut breaching),
                &mut FunctionCallScratch::new(),
            );
            assert_eq!(
                broken, fault,
                "{spelling} over a reader that breaks the contract by {breach:?}"
            );
        }
        unreadable += 4;
    }
    note!(
        "ACCEPT-NOTE: {CASE}: {unreadable} calls of MATCH, XMATCH, VLOOKUP, LOOKUP and XLOOKUP over a reference without a reader, with a refusing one, or with one that visits a cell outside the area or visits cells out of order ended in ReferenceUnreadable naming the argument"
    );

    // A reader may also visit blank cells, as Empty: every row's result over the test sheet is
    // the same over such a reader.
    let mut dense_rows = 0;
    for (spelling, values) in [
        (
            "MATCH",
            vec![
                CalcValue::number(4.0),
                reference(s, &["C7:C12"]).value,
                CalcValue::number(1.0),
            ],
        ),
        (
            "MATCH",
            vec![
                text_value("c"),
                reference(s, &["A21:A37"]).value,
                CalcValue::number(1.0),
            ],
        ),
        (
            "XMATCH",
            vec![CalcValue::empty(), reference(s, &["C7:C12"]).value],
        ),
        (
            "XMATCH",
            vec![
                CalcValue::number(99.0),
                reference(s, &["A1:A10"]).value,
                CalcValue::number(1.0),
                CalcValue::number(-1.0),
            ],
        ),
        (
            "XMATCH",
            vec![
                CalcValue::number(2.5),
                reference(s, &["C21:C26"]).value,
                CalcValue::number(1.0),
                CalcValue::number(-2.0),
            ],
        ),
        (
            "VLOOKUP",
            vec![
                CalcValue::number(3.0),
                reference(s, &["A9:C10"]).value,
                CalcValue::number(3.0),
                CalcValue::logical(false),
            ],
        ),
    ] {
        let target = surface(Notation::Call, spelling);
        let sparse = invoke(
            target,
            &mut Recorded::new(values.clone()),
            &mut EvalServices::with_cells(&fast, s),
            &mut FunctionCallScratch::new(),
        );
        let mut dense = DenseReader(s);
        let with_blanks = invoke(
            target,
            &mut Recorded::new(values),
            &mut EvalServices::with_cells(&fast, &mut dense),
            &mut FunctionCallScratch::new(),
        );
        assert!(sparse.is_ok(), "{spelling}: {sparse:?}");
        assert_eq!(
            sparse, with_blanks,
            "{spelling} over a reader that visits blank cells"
        );
        dense_rows += 1;
    }
    note!(
        "ACCEPT-NOTE: {CASE}: {dense_rows} calls gave the same results over a reader that also visits every blank cell as Empty"
    );

    // Blank cells sort after every value: XMATCH's next larger value of 99 in A40:A45 is its
    // first (searching from the first) or last (from the last) blank cell, cells the reader
    // never visits.
    let xmatch = surface(Notation::Call, "XMATCH");
    for (search, position) in [(1.0, 4.0), (-1.0, 6.0)] {
        s.visited = 0;
        let result = invoke(
            xmatch,
            &mut Recorded::new(vec![
                CalcValue::number(99.0),
                reference(s, &["A40:A45"]).value,
                CalcValue::number(1.0),
                CalcValue::number(search),
            ]),
            &mut EvalServices::with_cells(&fast, s),
            &mut FunctionCallScratch::new(),
        );
        assert_eq!(
            result,
            Ok(CalcValue::number(position)),
            "search_mode {search}"
        );
        assert_eq!(
            s.visited, 3,
            "the reader visits the 3 held cells of A40:A45"
        );
    }
    note!(
        "ACCEPT-NOTE: {CASE}: XMATCH(99,A40:A45,1) = 4 and XMATCH(99,A40:A45,1,-1) = 6, the first and last blank cells, with the reader visiting only the 3 held cells"
    );

    // NumericMode reaches the lookups only through text read as a number: under Exact a search
    // computes the value of its Fast row, and a text index is refused with ExactUnavailable.
    let exact = exact_mode();
    let under_exact = invoke(
        surface(Notation::Call, "MATCH"),
        &mut Recorded::new(vec![
            CalcValue::number(2.0),
            reference(s, &["A1:A6"]).value,
            CalcValue::number(1.0),
        ]),
        &mut EvalServices::with_cells(&exact, s),
        &mut FunctionCallScratch::new(),
    );
    assert_eq!(under_exact, Ok(CalcValue::number(4.0)));
    let index = surface(Notation::Call, "INDEX");
    let text_index = invoke(
        index,
        &mut Recorded::new(vec![
            reference(s, &["A1:C6"]).value,
            text_value("2"),
            CalcValue::number(1.0),
        ]),
        &mut EvalServices::with_cells(&exact, s),
        &mut FunctionCallScratch::new(),
    );
    assert_eq!(
        text_index,
        Err(FunctionFault::ExactUnavailable { id: index.id() })
    );
    note!(
        "ACCEPT-NOTE: {CASE}: under NumericMode::Exact MATCH(2,A1:A6,1) computes 4, its Fast value, and INDEX(A1:C6,\"2\",1) is refused with ExactUnavailable (reading text as a number rounds)"
    );

    assert_lookup_workbook(&rows, s);
    lookup_rows_note(&rows);
}

// ---------------------------------------------------------------------------------------------
// cfunc_m3_6__excel_comparison

/// What the rows of both lookup sheets publish: each row a result for its cell (calculated at its
/// tick with calc_func's value, or not modeled with its owner), and the evaluation record of the
/// rows' calls. No calculated row's value is a reference: a row whose result is one is owned
/// elsewhere ([`Gives::publication_owner`]).
fn lookup_published(run: &[&Rows]) -> (ResultSet, EvaluationRecord) {
    let source = format!("sha256:{}", sha256_hex(LOOKUP_WORKBOOK));
    let context = package_context(LOOKUP_WORKBOOK).fingerprint();
    let mut results = Vec::new();
    let mut evaluations = Vec::new();
    for rows in run {
        for row in &rows.oracle {
            let (provenance, value) = match (row.owner, &row.outcome) {
                (None, Ok(value)) => {
                    assert!(
                        value.as_reference().is_none() && value.as_array().is_none(),
                        "{}: a calculated row publishes calc_func's value as it is, never a reference or an array",
                        row.formula
                    );
                    (
                        Provenance::Calculated {
                            tick: row.tick,
                            revision: AUTHORED_REVISION,
                            context: context.clone(),
                            external_links: false,
                        },
                        Some(value.clone()),
                    )
                }
                (None, Err(fault)) => (
                    Provenance::Failed {
                        fault: format!("{fault:?}"),
                    },
                    None,
                ),
                (Some(owner), outcome) => (
                    Provenance::NotModeled {
                        feature: owner.feature(),
                        diagnostic: match outcome {
                            Ok(value) => format!("calc_func::invoke gives {}", shown_value(value)),
                            Err(fault) => format!("calc_func::invoke refuses: {fault:?}"),
                        },
                    },
                    None,
                ),
            };
            results.push(CellResult {
                cell: oracle_cell(rows.sheet, row.row),
                provenance,
                value,
            });
        }
        for call in &rows.calls {
            evaluations.push(Evaluation {
                cell: oracle_cell(rows.sheet, call.row),
                tick: call.tick,
                revision: AUTHORED_REVISION,
                context: context.clone(),
            });
        }
        for source in lookup_sources(rows.sheet) {
            let (row, column) = decode_a1(&source.a1);
            results.push(CellResult {
                cell: CellAddress::new(rows.sheet, row, column).expect("a cell of the grid"),
                provenance: Provenance::NotModeled {
                    feature: source.owner.feature(),
                    diagnostic: format!(
                        "calc_func's catalogue has no function of ={}; the test sheet's reader holds {} for this cell",
                        source.entered,
                        error_name(source.error)
                    ),
                },
                value: None,
            });
        }
    }
    (
        ResultSet {
            case: LOOKUP_CAPTURE_CASE.to_owned(),
            source: source.clone(),
            run: RunIdentity {
                revision: AUTHORED_REVISION,
                context,
            },
            results,
        },
        EvaluationRecord {
            case: LOOKUP_CAPTURE_CASE.to_owned(),
            source,
            evaluations,
        },
    )
}

/// Checks, for every row that computes a value, that the capture observed in the row's cell what
/// the row states Excel shows there: the value a measured row asserts (for a row published as not
/// modeled too, which the verdict does not compare), and what a row of [`Seen::Shape`] states
/// instead of its result; returns how many it checked.
fn assert_shown_as_stated(capture: &CaptureRecord, rows: &Rows) -> usize {
    let stated: Vec<(u32, Expect)> = SHOWN.with(|shown| {
        shown
            .borrow()
            .iter()
            .filter(|((sheet, _), _)| *sheet == rows.sheet)
            .map(|((_, row), (shows, _))| (*row, shows.clone()))
            .collect()
    });
    for (row, shows) in &stated {
        let cell = oracle_cell(rows.sheet, *row);
        let observed = capture
            .observations
            .iter()
            .find(|observation| observation.cell == cell)
            .unwrap_or_else(|| panic!("the capture has no observation of {}", cell.a1()));
        let Observation::ExcelRaw(value) = &observed.observation else {
            panic!(
                "{}: the capture's observation is not Excel's raw value",
                cell.a1()
            );
        };
        assert!(
            shows.holds(surface(Notation::Call, "INDEX").id(), &Ok(value.clone())),
            "{}!{}: the row states that Excel shows {}, the capture observed {}",
            rows.sheet,
            cell.a1(),
            shows.shown(),
            shown_value(value)
        );
    }
    stated.len()
}

/// [`judged`] for a lookup sheet: every cell as [`verdict_problems`] checks it, except the sheet's
/// source cells, whose verdicts must be not_modeled with their owners.
fn lookup_judged(
    capture: &CaptureRecord,
    results: &ResultSet,
    evaluations: &EvaluationRecord,
    rows: &Rows,
) -> Result<Comparison, JudgeError> {
    let policy = VerdictPolicy {
        numeric_mode: NumericMode::Fast,
        evidence: None,
    };
    let report = judge(capture, results, evaluations, &policy)?;
    let sources = lookup_sources(rows.sheet);
    let places: Vec<(u32, u32)> = sources.iter().map(|source| decode_a1(&source.a1)).collect();
    let mut problems = verdict_problems_beside(&report, rows, &places);
    for (source, (row, column)) in sources.iter().zip(&places) {
        let verdict = report
            .cells
            .iter()
            .find(|cell| {
                let at = &cell.formula.cell;
                at.sheet() == rows.sheet && at.row() == *row && at.column() == *column
            })
            .map(|cell| &cell.verdict);
        match verdict {
            Some(Verdict::NotModeled { feature, .. }) if *feature == source.owner.feature() => {}
            other => problems.push(format!(
                "{}!{} ={}: a source cell's verdict is {other:?}, not not_modeled with its owner",
                rows.sheet, source.a1, source.entered
            )),
        }
    }
    Ok(Comparison { report, problems })
}

/// Checks that the capture holds each source cell of the rows' sheet as a dynamic-array formula
/// with its formula, and observed in it the error the test sheet's reader holds for it; returns
/// how many it checked.
fn assert_sources_as_held(capture: &CaptureRecord, rows: &Rows) -> usize {
    let sources = lookup_sources(rows.sheet);
    for source in &sources {
        let (row, column) = decode_a1(&source.a1);
        let cell = CellAddress::new(rows.sheet, row, column).expect("a cell of the grid");
        let formula = capture
            .formulas
            .iter()
            .find(|formula| formula.cell == cell)
            .unwrap_or_else(|| panic!("the capture has no formula cell {}", source.a1));
        assert!(
            formula.dynamic_array
                && formula.formula.as_deref() == Some(format!("={}", source.entered).as_str()),
            "{}!{}: the capture holds {formula:?}, not the dynamic-array formula ={}",
            rows.sheet,
            source.a1,
            source.entered
        );
        let observed = capture
            .observations
            .iter()
            .find(|observation| observation.cell == cell)
            .unwrap_or_else(|| panic!("the capture has no observation of {}", source.a1));
        let Observation::ExcelRaw(value) = &observed.observation else {
            panic!(
                "{}: the capture's observation is not Excel's raw value",
                source.a1
            );
        };
        assert!(
            identical(value, &CalcValue::error(source.error)),
            "{}!{} ={}: the reader holds {}, the capture observed {}",
            rows.sheet,
            source.a1,
            source.entered,
            error_name(source.error),
            shown_value(value)
        );
    }
    sources.len()
}

#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test)]
fn cfunc_m3_6__excel_comparison() {
    const CASE: &str = "cfunc_m3_6__excel_comparison";
    let mut reference_sheet = reference_result_sheet();
    let reference_rows = reference_result_rows(&mut reference_sheet, true);
    let mut edges_sheet = lookup_edges_sheet();
    let edges_rows = lookup_edges_rows(&mut edges_sheet, true);
    let (results, evaluations) = lookup_published(&[&reference_rows, &edges_rows]);
    let count = |keep: &dyn Fn(&Provenance) -> bool| {
        results
            .results
            .iter()
            .filter(|result| keep(&result.provenance))
            .count()
    };
    let calculated = count(&|p| matches!(p, Provenance::Calculated { .. }));
    let engine_route = count(
        &|p| matches!(p, Provenance::NotModeled { feature, .. } if *feature == ENGINE_ROUTE.feature()),
    );
    let not_modeled = count(&|p| matches!(p, Provenance::NotModeled { .. }));
    note!(
        "ACCEPT-NOTE: {CASE}: the rows of both sheets publish {} results for {LOOKUP_WORKBOOK_PATH}: {calculated} calculated by calc_func::invoke; {engine_route} not modeled under dc-l10u (a reference result dereferenced or a nested call passed on, which only calc_fml::evaluate runs on the application's route); {} not modeled with another owner",
        results.results.len(),
        not_modeled - engine_route
    );
    if let Some(missing) = MissingCapture::recognize(LOOKUP_CAPTURE) {
        assert_eq!(
            (
                missing.case.as_str(),
                missing.fixture.as_str(),
                missing.command.as_str()
            ),
            (
                LOOKUP_CAPTURE_CASE,
                LOOKUP_WORKBOOK_PATH,
                LOOKUP_CAPTURE_COMMAND
            ),
            "the absent-capture marker names this capture"
        );
        note!("ACCEPT-NOT-RUN: {CASE}: {missing}");
        return;
    }
    let capture = CaptureRecord::decode(LOOKUP_CAPTURE).expect("the committed capture decodes");
    assert_eq!(capture.case, LOOKUP_CAPTURE_CASE);
    assert_eq!(capture.acquisition, Acquisition::LiveCapture);
    let session = capture
        .session
        .as_ref()
        .expect("the committed capture is a version 2 record");
    let workbook = format!("sha256:{}", sha256_hex(LOOKUP_WORKBOOK));
    if capture.source != workbook {
        note!(
            "ACCEPT-NOT-RUN: {CASE}: awaiting capture: the committed capture is of {}, and the oracle workbook as committed is {workbook} (cargo xtask capture --pending takes it)",
            capture.source
        );
        return;
    }
    let isolation: Vec<_> = session.failures_at(STAGE_ISOLATION).collect();
    assert!(
        isolation.is_empty(),
        "the committed capture records isolation failures: {isolation:?}"
    );
    let environment = capture
        .environment
        .as_ref()
        .expect("the capture records Excel's environment");
    for rows in [&reference_rows, &edges_rows] {
        let comparison = lookup_judged(&capture, &results, &evaluations, rows)
            .unwrap_or_else(|refusal| panic!("calc_verdict::judge refuses the capture: {refusal}"));
        assert!(
            comparison.problems.is_empty(),
            "{} problems on sheet {} of the verdict on Excel {}'s capture ({}):\n{}",
            comparison.problems.len(),
            rows.sheet,
            environment.target.excel_build(),
            comparison.report.summary(),
            comparison.problems.join("\n")
        );
        let stated = assert_shown_as_stated(&capture, rows);
        let held = assert_sources_as_held(&capture, rows);
        let conditional = rows
            .oracle
            .iter()
            .filter(|row| row.owner == Some(SOURCE_READ))
            .count();
        note!(
            "ACCEPT-NOTE: {CASE}: Excel comparison through calc_verdict::judge against Excel {}'s capture of the lookup oracle workbook: sheet {}: {}; for each of the sheet's {stated} rows that compute a value, published as calculated or not, the capture observed in the row's cell the value the row states Excel shows there ({conditional} of them conditional on source formulas); each of the sheet's {held} source cells is not modeled, a dynamic-array formula in the capture, and holds there the error the reader holds for it; {}",
            environment.target.excel_build(),
            rows.sheet,
            sheet_verdicts(&comparison.report, rows),
            comparison.report.summary()
        );
    }
}

// ---------------------------------------------------------------------------------------------
// dc-e00t: digit-bearing text that cannot be a number, date or time, read as text, through
// calc_func::invoke on catalogue targets.
//
// Text holding a word (a run of ASCII letters) that Excel's number, date and time recognition
// never reads, such as the labels 2016-17 Winter and Q1 2016, is no number: calc_func's coercion
// gives #VALUE! for it, a criterion holding it asks for text, and a range cell holding it does not
// meet = a number. The words the rule lets Excel read inside number text, as measured in Excel on
// this machine under the Windows locale en-ZA, are the month names and their prefixes of three
// letters or more, A, P, AM, PM, E and R (en-ZA's currency symbol); text holding only those words,
// or none, and text beyond printable ASCII stay refused as not modelled, owned by dc-uar.
//
// Each hand-written row asserts the value Excel gives, as captured, or, for a form the rule leaves
// undecided, the refusal beside the value Excel gives (1/2+0 states none: Excel reads 1/2 as a
// date in the year it calculates it in). Then each word of [`LABEL_WORDS`] in each
// form of [`LABEL_FORMS`] is read by + as "text"+0: a word marked no number gives #VALUE!, any
// other is refused, apart from 1e5, which is in the plain decimal form and reads as 100000.
// Comparisons over labels compared text before this bead too; their rows show that the labels
// keep doing so. `tests/fixtures/dc_e00t/oracle.xlsx`, written by the committed generator
// `tests/fixtures/make-fixtures.ps1`, holds every row's formula in column D of its sheet
// digit_labels_as_text, in the order the case runs its rows, beside the cells the rows read, and
// in column D of its sheet value_readings VALUE("text") of each generated text of the words of
// [`VALUE_SAMPLE_WORDS`], in the same order: Excel's second reading of those texts, which no row
// computes, since calc_func has no VALUE.
// `cargo xtask capture --fixture crates/calc-func/tests/fixtures/dc_e00t/oracle.xlsx --case
// dc_e00t__excel_comparison --out crates/calc-func/tests/captures/dc_e00t/oracle.capture.json`
// wrote Excel's capture of that workbook, and dc_e00t__excel_comparison publishes the rows'
// results and their recorded invoke calls to calc_verdict::judge against it: every computed row
// must be matched, every refused row not modeled with its owner, and every value a row states
// Excel gives must be the raw value the report carries for its cell. It publishes each
// value_readings cell as missing, and Excel's VALUE of each sampled text must be the value its
// "text"+0 row has, #VALUE! for every sampled text the rule decides.

/// The oracle workbook of dc-e00t, as committed.
const LABEL_WORKBOOK: &[u8] = include_bytes!("fixtures/dc_e00t/oracle.xlsx");
const LABEL_WORKBOOK_PATH: &str = "crates/calc-func/tests/fixtures/dc_e00t/oracle.xlsx";
/// Excel's capture of that workbook.
const LABEL_CAPTURE: &[u8] = include_bytes!("captures/dc_e00t/oracle.capture.json");
/// The case the capture is recorded under.
const LABEL_CAPTURE_CASE: &str = "dc_e00t__excel_comparison";
/// The oracle workbook's sheet that holds Excel's VALUE of each sampled generated text in column D.
const VALUE_READINGS_SHEET: &str = "value_readings";
/// Why the Excel comparison publishes no value for a cell of [`VALUE_READINGS_SHEET`].
const VALUE_READING_MISSING: &str = "calc_func has no VALUE: the cell is Excel's VALUE of a sampled generated dc-e00t text, which dc_e00t__excel_comparison reads beside the text's \"text\"+0 row";

/// The words the generated rows read, in the order the oracle workbook holds them, each with
/// whether it is a word no number text holds (so text holding it is no number) or one Excel reads
/// inside some number text (a month name or a prefix of one of three letters or more, A, P, AM,
/// PM, E or R), which leaves the text undecided.
const LABEL_WORDS: [(&str, bool); 88] = [
    ("a", false),
    ("b", true),
    ("c", true),
    ("d", true),
    ("e", false),
    ("f", true),
    ("g", true),
    ("h", true),
    ("i", true),
    ("j", true),
    ("k", true),
    ("l", true),
    ("m", true),
    ("n", true),
    ("o", true),
    ("p", false),
    ("q", true),
    ("r", false),
    ("s", true),
    ("t", true),
    ("u", true),
    ("v", true),
    ("w", true),
    ("x", true),
    ("y", true),
    ("z", true),
    ("R", false),
    ("AM", false),
    ("am", false),
    ("pm", false),
    ("ja", true),
    ("fe", true),
    ("ma", true),
    ("ap", true),
    ("ju", true),
    ("au", true),
    ("se", true),
    ("oc", true),
    ("no", true),
    ("de", true),
    ("jan", false),
    ("feb", false),
    ("mar", false),
    ("apr", false),
    ("may", false),
    ("jun", false),
    ("jul", false),
    ("aug", false),
    ("sep", false),
    ("oct", false),
    ("nov", false),
    ("dec", false),
    ("sept", false),
    ("JAN", false),
    ("january", false),
    ("february", false),
    ("march", false),
    ("april", false),
    ("june", false),
    ("july", false),
    ("august", false),
    ("september", false),
    ("october", false),
    ("november", false),
    ("december", false),
    ("janx", true),
    ("market", true),
    ("decade", true),
    ("monday", true),
    ("tuesday", true),
    ("wednesday", true),
    ("thursday", true),
    ("friday", true),
    ("saturday", true),
    ("sunday", true),
    ("mon", true),
    ("tue", true),
    ("wed", true),
    ("thu", true),
    ("fri", true),
    ("sat", true),
    ("sun", true),
    ("winter", true),
    ("summer", true),
    ("fy", true),
    ("noon", true),
    ("gmt", true),
    ("st", true),
];

/// The forms each word of [`LABEL_WORDS`] is read in, the word in place of the underscore: a date
/// with a month name, a time, an exponent, a currency symbol, numeric dates with / and -, digits
/// grouped by the capture's thousands separator (a space), a parenthesised negative, a percentage,
/// a currency amount, a weekday's long date and a fraction.
const LABEL_FORMS: [&str; 12] = [
    "1 _ 2020",
    "10:30 _",
    "1_5",
    "_1",
    "2020/1/2 _",
    "2020-1-2 _",
    "1 000 _",
    "(1) _",
    "1% _",
    "R1 _",
    "_, 1 January 2020",
    "1 1/2 _",
];

/// The words of [`LABEL_WORDS`] whose texts, in every form of [`LABEL_FORMS`], the oracle
/// workbook also reads with VALUE: a word no number text holds, a month name's prefix and a time
/// marker, so each form has a text the rule decides and texts it leaves undecided.
const VALUE_SAMPLE_WORDS: [&str; 3] = ["winter", "jan", "pm"];

/// A generated text: a word of [`LABEL_WORDS`] in a form of [`LABEL_FORMS`].
struct LabelText {
    text: String,
    /// The form's index in [`LABEL_FORMS`].
    form: usize,
    /// Whether the word is one no number text holds, so the rule decides the text.
    no_number: bool,
    /// Whether the word is one of [`VALUE_SAMPLE_WORDS`], so the oracle workbook reads the text
    /// with VALUE too.
    sampled: bool,
}

/// The generated texts, in the order the oracle workbook holds them: each word of [`LABEL_WORDS`]
/// in each form of [`LABEL_FORMS`].
fn label_texts() -> Vec<LabelText> {
    let mut texts = Vec::new();
    for (word, no_number) in LABEL_WORDS {
        for (form, pattern) in LABEL_FORMS.iter().enumerate() {
            texts.push(LabelText {
                text: pattern.replace('_', word),
                form,
                no_number,
                sampled: VALUE_SAMPLE_WORDS.contains(&word),
            });
        }
    }
    texts
}

/// The behaviour the refused dc-e00t rows show: text the rule leaves undecided.
const UNDECIDED_TEXT: Owner = NUMBER_TEXT;

/// The cells the dc-e00t rows read. A1:A8 hold labels holding digits, as the criteria ranges of
/// the Ofgem price-cap workbooks do, beside the number 2016 (A5) and the text 2016 (A6); A7 is A1
/// in lower case. B1:B8 hold the powers of two 1 to 128, so a sum names the rows it added. C1 and
/// C2 hold labels as criteria; C3:C10 hold forms the rule leaves undecided (a date, a time, an
/// amount in R, a label with an en dash, the text 2016-17 and the fractions 1 1/2 and 1/2) or
/// decides (a label of 314 characters).
fn digit_labels_sheet() -> TestSheet {
    let long = vec!["2016-17 Winter"; 21].join(" ");
    let mut cells: Vec<(String, String, CalcValue)> = Vec::new();
    for (address, text) in [
        ("A1", "2016-17 Winter"),
        ("A2", "2047-48 Summer"),
        ("A3", "2047-48 Winter"),
        ("A4", "Q1 2016"),
        ("A6", "2016"),
        ("A7", "2016-17 winter"),
        ("A8", "2063-2064 Winter"),
        ("C1", "2016-17 Winter"),
        ("C2", "Q1 2016"),
        ("C3", "1 Jan 2020"),
        ("C4", "10:30 am"),
        ("C5", "R1"),
        ("C6", "2016\u{2013}17 Winter"),
        ("C8", "2016-17"),
        ("C9", "1 1/2"),
        ("C10", "1/2"),
    ] {
        cells.push((
            address.to_owned(),
            format!("text {text:?}"),
            text_value(text),
        ));
    }
    cells.push((
        "C7".to_owned(),
        format!(
            "text of {} characters, 2016-17 Winter 21 times with spaces",
            long.len()
        ),
        text_value(&long),
    ));
    cells.push((
        "A5".to_owned(),
        "2016".to_owned(),
        CalcValue::number(2016.0),
    ));
    for row in 1..=8u32 {
        let power = 2f64.powi(row as i32 - 1);
        cells.push((
            format!("B{row}"),
            format!("{power}"),
            CalcValue::number(power),
        ));
    }
    let cells: Vec<(&str, &str, CalcValue)> = cells
        .iter()
        .map(|(address, entry, value)| (address.as_str(), entry.as_str(), value.clone()))
        .collect();
    TestSheet::new(&cells)
}

/// What a dc-e00t row expects of calc_func::invoke.
enum LabelOutcome {
    /// This value, which is the value Excel gives.
    Gives(Expect),
    /// A refusal as not modelled naming this argument, owned by [`UNDECIDED_TEXT`]; Excel gives
    /// this value, when the row states it (a generated row does not, nor the hand-written row
    /// 1/2+0, whose value in Excel is a date in the year Excel calculates it in).
    Undecided(usize, Option<Expect>),
}

/// The dc-e00t rows, with the value each states Excel gives, in row order.
struct LabelRows {
    rows: Rows,
    excel: Vec<Option<Expect>>,
}

/// Runs one dc-e00t row: `surface` on `args` through calc_func::invoke under Fast, asserted to
/// have `outcome`. Records the call and the row, with the next cell of column D (a refused row
/// owned by [`UNDECIDED_TEXT`]), and the value it states Excel gives, and prints the row when
/// `noted` (a generated row is counted in a note of its form instead).
fn label_check(
    label_rows: &mut LabelRows,
    sheet: &mut TestSheet,
    surface: (Notation, &str),
    args: &[Arg],
    outcome: LabelOutcome,
    noted: bool,
) {
    let rows = &mut label_rows.rows;
    let (expect, owner, excel) = match outcome {
        LabelOutcome::Gives(expect) => (expect.clone(), None, Some(expect)),
        LabelOutcome::Undecided(argument, excel) => {
            (Refused(Some(argument)), Some(UNDECIDED_TEXT), excel)
        }
    };
    let (formula, _, result, tick) =
        call_and_assert(sheet, &NumericMode::Fast, surface, args, &expect);
    let row = u32::try_from(rows.oracle.len() + 1).expect("a row number");
    if !rows.quiet && noted {
        let reads: Vec<&str> = args
            .iter()
            .flat_map(|a| a.reads.iter().map(String::as_str))
            .collect();
        let cells = if reads.is_empty() {
            String::new()
        } else {
            format!(" with {}", reads.join(", "))
        };
        let shown = match (owner, &excel) {
            (None, _) => format!("{} (Excel's value, as captured)", expect.shown()),
            (Some(owner), Some(excel)) => format!(
                "refused as not modelled: {result:?} (owned by {}); Excel gives {} (as captured)",
                owner.feature(),
                excel.shown()
            ),
            (Some(owner), None) => format!(
                "refused as not modelled: {result:?} (owned by {}); the row states no value of Excel's, which depends on the year Excel calculates it in",
                owner.feature()
            ),
        };
        note!(
            "ACCEPT-NOTE: {}: oracle {}!{} {formula}{cells} -> {shown}",
            rows.case,
            rows.sheet,
            encode_a1(row, FORMULA_COLUMN)
        );
    }
    rows.calls.push(Call { row, tick });
    rows.oracle.push(OracleRow {
        row,
        formula,
        outcome: result,
        choice: None,
        owner,
        tick,
    });
    label_rows.excel.push(excel);
}

/// Runs a dc-e00t row that computes the value Excel gives:
/// `label!(rows, sheet, surface, [args], expect)`.
macro_rules! label {
    ($rows:expr, $sheet:expr, $surface:expr, [$($arg:expr),* $(,)?], $expect:expr) => {{
        let args = [$($arg),*];
        label_check($rows, $sheet, $surface, &args, LabelOutcome::Gives($expect), true)
    }};
}

/// Runs a dc-e00t row on a form the rule leaves undecided, which calc_func refuses naming
/// `argument`: `undecided!(rows, sheet, surface, [args], argument, excel)`, `excel` the value
/// Excel gives, as captured.
macro_rules! undecided {
    ($rows:expr, $sheet:expr, $surface:expr, [$($arg:expr),* $(,)?], $argument:expr, $excel:expr) => {{
        let args = [$($arg),*];
        label_check(
            $rows,
            $sheet,
            $surface,
            &args,
            LabelOutcome::Undecided($argument, Some($excel)),
            true,
        )
    }};
}

/// The rows of dc_e00t__digit_labels_as_text, run through calc_func::invoke on `s`, in the order
/// the oracle workbook's sheet holds their formulas.
fn digit_label_rows(s: &mut TestSheet, quiet: bool) -> LabelRows {
    const CASE: &str = "dc_e00t__digit_labels_as_text";
    let mut rows = LabelRows {
        rows: Rows::new(CASE, "digit_labels_as_text", quiet),
        excel: Vec::new(),
    };
    let r = &mut rows;
    use WorksheetErrorCode::Value;
    let a = |s: &TestSheet| reference(s, &["A1:A8"]);
    let b = |s: &TestSheet| reference(s, &["B1:B8"]);
    let at = |s: &TestSheet, a1: &str| reference(s, &[a1]);

    // A label read as a number is #VALUE!, on either side of an operator, as IF's condition and
    // as a direct argument of SUM; COUNT does not count it.
    label!(r, s, PLUS, [cell(s, "A1"), num("0")], E(Value));
    label!(r, s, PLUS, [num("0"), cell(s, "A4")], E(Value));
    label!(r, s, TIMES, [cell(s, "A2"), num("2")], E(Value));
    label!(r, s, NEGATE, [cell(s, "A3")], E(Value));
    label!(r, s, PERCENT, [cell(s, "A8")], E(Value));
    label!(r, s, POWER, [cell(s, "A1"), num("1")], E(Value));
    label!(r, s, PLUS, [cell(s, "C7"), num("0")], E(Value));
    label!(r, s, IF, [cell(s, "A1"), num("1"), num("2")], E(Value));
    label!(r, s, SUM, [txt("Q1 2016"), num("1")], E(Value));
    label!(r, s, named("COUNT"), [txt("Q1 2016"), num("1")], N(1.0));

    // Text written like a date or a time that holds a word no number text holds: a date and time
    // joined by T, a.m. written with stops, an ordinal day and a zone letter.
    label!(r, s, PLUS, [txt("2020-01-02T10:30"), num("0")], E(Value));
    label!(r, s, PLUS, [txt("10:30 a.m."), num("0")], E(Value));
    label!(r, s, PLUS, [txt("1st January 2020"), num("0")], E(Value));
    label!(r, s, PLUS, [txt("10:30Z"), num("0")], E(Value));

    // Comparisons over labels compare text, ASCII case ignored, and text orders after numbers.
    label!(r, s, EQ, [cell(s, "A1"), cell(s, "A7")], L(true));
    label!(r, s, EQ, [cell(s, "A1"), cell(s, "C1")], L(true));
    label!(r, s, NE, [cell(s, "A1"), cell(s, "A3")], L(true));
    label!(r, s, EQ, [cell(s, "A4"), num("2016")], L(false));
    label!(r, s, GT, [cell(s, "A1"), cell(s, "A5")], L(true));

    // A criterion holding a label asks for that text (A1 and A7); a label in the range does not
    // meet = 2016, which the number 2016 and the text 2016 meet.
    label!(
        r,
        s,
        named("COUNTIF"),
        [a(s), txt("2016-17 Winter")],
        N(2.0)
    );
    label!(r, s, named("COUNTIF"), [a(s), joined(s, "=", "C1")], N(2.0));
    label!(r, s, named("COUNTIF"), [a(s), at(s, "C1")], N(2.0));
    label!(
        r,
        s,
        named("COUNTIF"),
        [a(s), joined(s, "<>", "C1")],
        N(6.0)
    );
    label!(r, s, named("COUNTIF"), [a(s), txt("Q1 2016")], N(1.0));
    label!(r, s, named("COUNTIF"), [a(s), at(s, "C2")], N(1.0));
    label!(r, s, named("COUNTIF"), [a(s), num("2016")], N(2.0));
    label!(r, s, named("COUNTIF"), [a(s), txt("=2016")], N(2.0));
    label!(
        r,
        s,
        named("SUMIF"),
        [a(s), joined(s, "=", "C1"), b(s)],
        N(65.0)
    );
    label!(r, s, named("SUMIF"), [a(s), num("2016"), b(s)], N(48.0));
    label!(
        r,
        s,
        named("SUMIFS"),
        [b(s), a(s), joined(s, "=", "C1")],
        N(65.0)
    );
    label!(
        r,
        s,
        named("SUMIFS"),
        [b(s), a(s), joined(s, "=", "A2")],
        N(2.0)
    );
    label!(
        r,
        s,
        named("SUMIFS"),
        [
            b(s),
            a(s),
            joined(s, "<>", "C1"),
            a(s),
            joined(s, "<>", "A2")
        ],
        N(188.0)
    );
    label!(
        r,
        s,
        named("AVERAGEIF"),
        [a(s), joined(s, "=", "C1"), b(s)],
        N(32.5)
    );
    label!(
        r,
        s,
        named("AVERAGEIFS"),
        [b(s), a(s), joined(s, "=", "C1")],
        N(32.5)
    );
    label!(
        r,
        s,
        named("COUNTIFS"),
        [a(s), joined(s, "=", "C1"), b(s), txt(">1")],
        N(1.0)
    );
    label!(
        r,
        s,
        named("MAXIFS"),
        [b(s), a(s), joined(s, "=", "C1")],
        N(64.0)
    );
    label!(
        r,
        s,
        named("MINIFS"),
        [b(s), a(s), joined(s, "=", "C1")],
        N(1.0)
    );

    // Forms the rule leaves undecided stay refused: a date with a month name, a time, an amount in
    // R, text beyond printable ASCII and text holding no word, as an operand, as a criterion, as a
    // range cell under = a number and as IF's condition.
    undecided!(r, s, PLUS, [cell(s, "C3"), num("0")], 0, N(43831.0));
    undecided!(r, s, PLUS, [cell(s, "C4"), num("0")], 0, N(0.4375));
    undecided!(r, s, PLUS, [cell(s, "C5"), num("0")], 0, N(1.0));
    undecided!(r, s, PLUS, [cell(s, "C6"), num("0")], 0, E(Value));
    undecided!(r, s, PLUS, [cell(s, "C8"), num("0")], 0, E(Value));
    undecided!(
        r,
        s,
        named("COUNTIF"),
        [a(s), joined(s, "=", "C3")],
        1,
        N(0.0)
    );
    undecided!(r, s, named("COUNTIF"), [a(s), at(s, "C8")], 1, N(0.0));
    undecided!(
        r,
        s,
        named("COUNTIF"),
        [reference(s, &["C3:C8"]), num("2016")],
        0,
        N(0.0)
    );
    // IF reads no number in its condition: digit text there is #VALUE!, whatever it reads as.
    label!(r, s, IF, [cell(s, "C3"), num("1"), num("2")], E(Value));
    // Fractions: 1 1/2, and 1/2, as an operand, as a criterion and as a criterion whose ranges
    // have one shape. As an operand, 1/2 states no value: Excel reads it in this locale as a date,
    // 2 January of the year Excel calculates it in, so its value changes with the year.
    undecided!(r, s, PLUS, [cell(s, "C9"), num("0")], 0, N(1.5));
    label_check(
        r,
        s,
        PLUS,
        &[cell(s, "C10"), num("0")],
        LabelOutcome::Undecided(0, None),
        true,
    );
    undecided!(r, s, named("COUNTIF"), [a(s), at(s, "C9")], 1, N(0.0));
    undecided!(r, s, named("COUNTIF"), [a(s), at(s, "C10")], 1, N(0.0));
    undecided!(r, s, named("SUMIFS"), [b(s), a(s), txt("1/2")], 2, N(0.0));

    // Ranges of different shapes give #VALUE! beside the criterion 1/2, which the row above
    // refuses: the shapes are compared before any criterion is read.
    label!(
        r,
        s,
        named("SUMIFS"),
        [b(s), reference(s, &["A1:A7"]), txt("1/2")],
        E(Value)
    );

    // Each word in each form, read by +: a word no number text holds makes the text #VALUE!; text
    // holding only words Excel reads is refused, apart from 1e5, which is in the plain decimal
    // form and reads as 100000.
    for LabelText {
        text, no_number, ..
    } in label_texts()
    {
        let args = [txt(&text), num("0")];
        let outcome = if no_number {
            LabelOutcome::Gives(E(Value))
        } else if text == "1e5" {
            LabelOutcome::Gives(N(100_000.0))
        } else {
            LabelOutcome::Undecided(0, None)
        };
        label_check(r, s, PLUS, &args, outcome, false);
    }
    rows
}

/// Asserts that the oracle workbook `package`'s sheet [`VALUE_READINGS_SHEET`] holds exactly
/// VALUE("text") of each of `texts` in column D, in order, and nothing else.
fn assert_value_readings_sheet(package: &[u8], texts: &[&LabelText]) {
    let parts = stored_parts(package);
    let workbook = part_text(&parts, "xl/workbook.xml");
    let relationships = part_text(&parts, "xl/_rels/workbook.xml.rels");
    let id = start_tags(workbook, "sheet")
        .into_iter()
        .find(|tag| attribute(tag, "name") == Some(VALUE_READINGS_SHEET))
        .and_then(|tag| attribute(tag, "r:id"))
        .unwrap_or_else(|| panic!("the oracle workbook lists no sheet {VALUE_READINGS_SHEET}"));
    let target = start_tags(relationships, "Relationship")
        .into_iter()
        .find(|tag| attribute(tag, "Id") == Some(id))
        .and_then(|tag| attribute(tag, "Target"))
        .unwrap_or_else(|| panic!("the oracle workbook has no relationship {id}"));
    let cells = worksheet_cells(part_text(&parts, &format!("xl/{target}")));
    let expected: BTreeMap<(u32, u32), String> = texts
        .iter()
        .enumerate()
        .map(|(index, text)| {
            let row = u32::try_from(index + 1).expect("a row number");
            ((row, FORMULA_COLUMN), format!("VALUE(\"{}\")", text.text))
        })
        .collect();
    let found: BTreeMap<(u32, u32), String> = cells
        .into_iter()
        .map(|(place, cell)| match cell {
            FixtureCell::Formula(formula) => (place, formula),
            FixtureCell::DynamicArray(formula) => panic!(
                "{VALUE_READINGS_SHEET}!{}: a dynamic-array formula {formula:?}",
                encode_a1(place.0, place.1)
            ),
            FixtureCell::Constant(value) => panic!(
                "{VALUE_READINGS_SHEET}!{}: a constant {value:?}",
                encode_a1(place.0, place.1)
            ),
        })
        .collect();
    assert!(
        found == expected,
        "the oracle workbook's sheet {VALUE_READINGS_SHEET} does not hold exactly VALUE of each generated text in column D, in order"
    );
}

#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test)]
fn dc_e00t__digit_labels_as_text() {
    const CASE: &str = "dc_e00t__digit_labels_as_text";
    let mut sheet = digit_labels_sheet();
    note!("ACCEPT-NOTE: {CASE}: the sheet holds {}", sheet.described());
    let LabelRows { rows, excel } = digit_label_rows(&mut sheet, false);
    use WorksheetErrorCode::Value;
    assert!(
        rows.oracle
            .iter()
            .all(|row| row.owner.is_some() == row.outcome.is_err()),
        "a refused row names its owner, a computed row none"
    );
    assert!(
        rows.oracle
            .iter()
            .filter_map(|row| row.owner)
            .all(|owner| owner == UNDECIDED_TEXT),
        "every refused row is owned by {}",
        UNDECIDED_TEXT.bead
    );
    let texts = label_texts();
    let generated = texts.len();
    assert_eq!(generated, LABEL_WORDS.len() * LABEL_FORMS.len());
    let hand = rows.oracle.len() - generated;
    let (hand_rows, generated_rows) = rows.oracle.split_at(hand);
    let refused = |rows: &[OracleRow]| rows.iter().filter(|row| row.outcome.is_err()).count();
    let decided_words = LABEL_WORDS.iter().filter(|(_, no)| *no).count();
    assert_eq!(
        generated_rows.len() - refused(generated_rows),
        decided_words * LABEL_FORMS.len() + 1,
        "every form of a word no number text holds computes #VALUE!, 1e5 computes 100000, and every other is refused"
    );
    assert_eq!(excel.len(), rows.oracle.len());
    // Each generated row, counted by form: computed #VALUE!, computed as a number, refused.
    let mut by_form = [(0usize, 0usize, 0usize); LABEL_FORMS.len()];
    for (text, row) in texts.iter().zip(generated_rows) {
        assert_eq!(row.formula, format!("=\"{}\"+0", text.text));
        let counts = &mut by_form[text.form];
        match &row.outcome {
            Ok(value) if value.as_error() == Some(Value) => counts.0 += 1,
            Ok(value) => {
                assert_eq!(value.as_number(), Some(100_000.0), "{}", row.formula);
                counts.1 += 1;
            }
            Err(_) => counts.2 += 1,
        }
    }
    note!(
        "ACCEPT-NOTE: {CASE}: the generated rows by form (#VALUE! for a word no number text holds / computed as a number / refused): {}",
        LABEL_FORMS
            .iter()
            .zip(by_form)
            .map(|(form, (errors, numbers, refused))| format!(
                "{form:?} {errors}/{numbers}/{refused}"
            ))
            .collect::<Vec<_>>()
            .join(", ")
    );

    // The oracle workbook: its sheet holds exactly the rows' formulas and cells, its sheet
    // value_readings exactly VALUE of each sampled generated text, and the committed workbook is
    // its generator's output.
    assert_workbook_sheet(LABEL_WORKBOOK, LABEL_WORKBOOK_PATH, &rows, &sheet);
    let sampled: Vec<&LabelText> = texts.iter().filter(|text| text.sampled).collect();
    assert_eq!(
        sampled.len(),
        VALUE_SAMPLE_WORDS.len() * LABEL_FORMS.len(),
        "each sample word is a word of the generated rows"
    );
    assert_value_readings_sheet(LABEL_WORKBOOK, &sampled);
    note!(
        "ACCEPT-NOTE: {CASE}: {LABEL_WORKBOOK_PATH}: sheet {VALUE_READINGS_SHEET} holds exactly VALUE(\"text\") of each of the {} generated texts of the words {} (each of the {} forms), in column D, in the order of their \"text\"+0 rows, and no other cell",
        sampled.len(),
        VALUE_SAMPLE_WORDS.join(", "),
        LABEL_FORMS.len()
    );
    #[cfg(all(windows, not(target_arch = "wasm32")))]
    {
        let written = generated_fixtures();
        let names: Vec<&str> = written.keys().map(String::as_str).collect();
        assert_eq!(
            names, GENERATED_WORKBOOKS,
            "the generator writes the oracle workbooks and nothing else"
        );
        let regenerated = &written["dc_e00t/oracle.xlsx"];
        assert!(
            regenerated.as_slice() == LABEL_WORKBOOK,
            "the committed dc-e00t oracle workbook is not the generator's output (sha256 {} committed, {} generated)",
            sha256_hex(LABEL_WORKBOOK),
            sha256_hex(regenerated)
        );
        note!(
            "ACCEPT-NOTE: {CASE}: crates/calc-func/tests/fixtures/make-fixtures.ps1, run now under Windows PowerShell, wrote the committed dc-e00t oracle workbook byte for byte ({} bytes)",
            regenerated.len()
        );
    }
    #[cfg(not(all(windows, not(target_arch = "wasm32"))))]
    {
        note!(
            "ACCEPT-NOTE: {CASE}: the generator runs under Windows PowerShell, so the check that the committed oracle workbook is its output runs on the windows route; here the workbook is checked against the rows only"
        );
    }
    note!(
        "ACCEPT-NOTE: {CASE}: {} rows through calc_func::invoke: {hand} written by hand, of which {} compute the value Excel gives (the labels as operands, as IF's condition, as SUM's and COUNT's direct arguments, in comparisons, and as criteria and range cells of COUNTIF, COUNTIFS, SUMIF, SUMIFS, AVERAGEIF, AVERAGEIFS, MAXIFS and MINIFS, text written like a date or a time that holds such a word, and SUMIFS over ranges of different shapes beside an undecided criterion) and {} are refused (a date with a month name, a time, an amount in R, a label with an en dash, the text 2016-17 and the fractions 1 1/2 and 1/2), each beside the value Excel gives apart from 1/2+0, whose value in Excel is a date in the year Excel calculates it in; and {generated} generated, {} words in {} forms as \"text\"+0, of which {} hold a word no number text holds and give #VALUE!, 1e5 gives 100000 and {} are refused; every refusal is owned by {}",
        rows.oracle.len(),
        hand - refused(hand_rows),
        refused(hand_rows),
        LABEL_WORDS.len(),
        LABEL_FORMS.len(),
        decided_words * LABEL_FORMS.len(),
        refused(generated_rows),
        UNDECIDED_TEXT.feature()
    );
}

/// Whether an observed value is the value `expect` states.
fn states(expect: &Expect, value: &CalcValue) -> bool {
    match expect {
        N(n) => value.as_number().map(f64::to_bits) == Some(n.to_bits()),
        L(b) => value.as_logical() == Some(*b),
        Expect::Text(text) => value.as_text() == Some(&ExcelText::from_interop_assignment(text)),
        Expect::Units(units) => {
            value.as_text().map(ExcelText::utf16_code_units) == Some(units.as_slice())
        }
        E(code) => value.as_error() == Some(*code),
        Refused(_) | Expect::Required(_) => false,
    }
}

#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test)]
fn dc_e00t__excel_comparison() {
    const CASE: &str = "dc_e00t__excel_comparison";
    let LabelRows { rows, excel } = digit_label_rows(&mut digit_labels_sheet(), true);
    let (mut results, evaluations) = published_in(LABEL_WORKBOOK, LABEL_CAPTURE_CASE, &[&rows]);
    assert_eq!(
        (results.results.len(), evaluations.evaluations.len()),
        (rows.oracle.len(), rows.oracle.len()),
        "a result and an evaluation for each row"
    );
    // The value_readings cells: calc_func computes none of them, so each is published as missing.
    let texts = label_texts();
    let sampled: Vec<(usize, &LabelText)> = texts
        .iter()
        .enumerate()
        .filter(|(_, text)| text.sampled)
        .collect();
    for index in 0..sampled.len() {
        let row = u32::try_from(index + 1).expect("a row number");
        results.results.push(CellResult {
            cell: oracle_cell(VALUE_READINGS_SHEET, row),
            provenance: Provenance::Missing {
                reason: VALUE_READING_MISSING.to_owned(),
            },
            value: None,
        });
    }
    let not_modelled = results
        .results
        .iter()
        .filter(|result| matches!(result.provenance, Provenance::NotModeled { .. }))
        .count();
    assert_eq!(
        not_modelled,
        rows.refused(),
        "every refused row is published as not modelled, and only those"
    );
    let capture = CaptureRecord::decode(LABEL_CAPTURE).expect("the committed capture decodes");
    assert_eq!(capture.case, LABEL_CAPTURE_CASE);
    assert_eq!(capture.acquisition, Acquisition::LiveCapture);
    let session = capture
        .session
        .as_ref()
        .expect("the committed capture is a version 2 record");
    let workbook = format!("sha256:{}", sha256_hex(LABEL_WORKBOOK));
    if capture.source != workbook {
        note!(
            "ACCEPT-NOT-RUN: {CASE}: awaiting capture: the committed capture is of {}, and the oracle workbook as committed is {workbook} (cargo xtask capture --pending takes it)",
            capture.source
        );
        return;
    }
    let isolation: Vec<_> = session.failures_at(STAGE_ISOLATION).collect();
    assert!(
        isolation.is_empty(),
        "the committed capture records isolation failures: {isolation:?}"
    );
    let environment = capture
        .environment
        .as_ref()
        .expect("the capture records Excel's environment");
    let comparison = judged(&capture, &results, &evaluations, &rows)
        .unwrap_or_else(|refusal| panic!("calc_verdict::judge refuses the capture: {refusal}"));
    assert!(
        comparison.problems.is_empty(),
        "{} problems on the verdict on Excel {}'s capture ({}):\n{}",
        comparison.problems.len(),
        environment.target.excel_build(),
        comparison.report.summary(),
        comparison.problems.join("\n")
    );

    // The value each row states Excel gives (the expected value of a computed row, the value
    // beside a refused hand-written row) is the raw value the report carries for the row's cell.
    let observed: BTreeMap<u32, &Observation> = comparison
        .report
        .cells
        .iter()
        .filter(|cell| {
            cell.formula.cell.sheet() == rows.sheet && cell.formula.cell.column() == FORMULA_COLUMN
        })
        .filter_map(|cell| Some((cell.formula.cell.row(), cell.observation.as_ref()?)))
        .collect();
    assert_eq!(observed.len(), rows.oracle.len(), "an observation per row");
    let hand = rows.oracle.len() - texts.len();
    let (mut stated, mut unstated_numbers, mut unstated_errors) = (0, 0, 0);
    let mut unstated_hand = Vec::new();
    let mut plus_readings = Vec::new();
    for (index, (row, excel)) in rows.oracle.iter().zip(&excel).enumerate() {
        let at = format!("{}!{}", rows.sheet, encode_a1(row.row, FORMULA_COLUMN));
        let Some(Observation::ExcelRaw(value)) = observed.get(&row.row) else {
            panic!("{at}: the report carries no raw value of Excel's");
        };
        plus_readings.push(value);
        match excel {
            Some(expect) => {
                assert!(
                    states(expect, value),
                    "{at} {}: the row says Excel gives {}, the capture holds {}",
                    row.formula,
                    expect.shown(),
                    shown_value(value)
                );
                stated += 1;
            }
            // A hand-written row that states no value (1/2+0): Excel reads the text as a date,
            // which is a number in any year.
            None if index < hand => {
                assert!(
                    value.as_number().is_some(),
                    "{at} {}: Excel gives {}, not a date",
                    row.formula,
                    shown_value(value)
                );
                unstated_hand.push(format!("{at} {} {}", row.formula, shown_value(value)));
            }
            None if value.as_number().is_some() => unstated_numbers += 1,
            None => {
                assert_eq!(
                    value.as_error(),
                    Some(WorksheetErrorCode::Value),
                    "{at} {}: Excel gives {}",
                    row.formula,
                    shown_value(value)
                );
                unstated_errors += 1;
            }
        }
    }
    note!(
        "ACCEPT-NOTE: {CASE}: the value each of {stated} rows states Excel gives is Excel's raw value for its cell on the report, the refused hand-written rows' included; {} hand-written row states none, its value in Excel being a date in the year Excel calculates it in, and Excel reads it as a number ({}); of the {} generated rows the rule leaves undecided, Excel reads {unstated_numbers} as numbers and gives #VALUE! for {unstated_errors}",
        unstated_hand.len(),
        unstated_hand.join("; "),
        unstated_numbers + unstated_errors
    );

    // Each generated text's "text"+0 row, by form: texts the rule decides (#VALUE! in Excel, as
    // each row states), other texts Excel reads as numbers (1e5, which calc_func reads as 100000,
    // among them), other texts Excel gives #VALUE! for.
    let generated_plus = &plus_readings[hand..];
    let mut by_form = [(0usize, 0usize, 0usize); LABEL_FORMS.len()];
    for (text, plus) in texts.iter().zip(generated_plus) {
        let counts = &mut by_form[text.form];
        if text.no_number {
            assert_eq!(plus.as_error(), Some(WorksheetErrorCode::Value));
            counts.0 += 1;
        } else if plus.as_number().is_some() {
            counts.1 += 1;
        } else {
            counts.2 += 1;
        }
    }
    note!(
        "ACCEPT-NOTE: {CASE}: Excel's \"text\"+0 of the {} generated texts by form (texts the rule decides, #VALUE! / other texts Excel reads as numbers, 1e5 among them / other texts Excel gives #VALUE! for): {}",
        texts.len(),
        LABEL_FORMS
            .iter()
            .zip(by_form)
            .map(|(form, (decided, numbers, errors))| format!(
                "{form:?} {decided}/{numbers}/{errors}"
            ))
            .collect::<Vec<_>>()
            .join(", ")
    );

    // Excel's second reading: VALUE of each sampled generated text, on sheet value_readings,
    // published as missing, is the value its "text"+0 row has (numbers bit for bit), so #VALUE!
    // for every sampled text the rule decides.
    let mut readings: BTreeMap<u32, &CalcValue> = BTreeMap::new();
    for cell in &comparison.report.cells {
        let address = &cell.formula.cell;
        if address.sheet() != VALUE_READINGS_SHEET {
            continue;
        }
        let at = format!("{VALUE_READINGS_SHEET}!{}", address.a1());
        assert_eq!(address.column(), FORMULA_COLUMN, "{at}: outside column D");
        assert!(
            matches!(&cell.verdict, Verdict::Missing { reason: Some(reason) } if reason == VALUE_READING_MISSING),
            "{at}: {:?}, not missing for the published reason",
            cell.verdict
        );
        let Some(Observation::ExcelRaw(value)) = cell.observation.as_ref() else {
            panic!("{at}: the report carries no raw value of Excel's");
        };
        readings.insert(address.row(), value);
    }
    assert_eq!(
        readings.len(),
        sampled.len(),
        "a VALUE reading per sampled generated text"
    );
    // By form: decided texts (#VALUE! in both readings), undecided texts Excel reads as numbers in
    // both readings, undecided texts #VALUE! in both.
    let mut by_form = [(0usize, 0usize, 0usize); LABEL_FORMS.len()];
    for (index, (generated, text)) in sampled.iter().enumerate() {
        let row = u32::try_from(index + 1).expect("a row number");
        let value = readings[&row];
        let plus = generated_plus[*generated];
        assert!(
            identical(value, plus),
            "{VALUE_READINGS_SHEET}!{}: Excel's VALUE({:?}) is {}, its +0 {}",
            encode_a1(row, FORMULA_COLUMN),
            text.text,
            shown_value(value),
            shown_value(plus)
        );
        let counts = &mut by_form[text.form];
        if text.no_number {
            assert_eq!(
                value.as_error(),
                Some(WorksheetErrorCode::Value),
                "{VALUE_READINGS_SHEET}!{}: the rule decides {:?}, Excel's VALUE of it is {}",
                encode_a1(row, FORMULA_COLUMN),
                text.text,
                shown_value(value)
            );
            counts.0 += 1;
        } else if value.as_number().is_some() {
            counts.1 += 1;
        } else {
            counts.2 += 1;
        }
    }
    note!(
        "ACCEPT-NOTE: {CASE}: Excel's VALUE of each of the {} generated texts of the words {} (sheet {VALUE_READINGS_SHEET}, each published as missing: calc_func has no VALUE) is the raw value of its \"text\"+0 row, numbers bit for bit; by form (texts the rule decides, #VALUE! in both readings / undecided texts Excel reads as numbers / undecided texts Excel gives #VALUE! for): {}",
        sampled.len(),
        VALUE_SAMPLE_WORDS.join(", "),
        LABEL_FORMS
            .iter()
            .zip(by_form)
            .map(|(form, (decided, numbers, errors))| format!(
                "{form:?} {decided}/{numbers}/{errors}"
            ))
            .collect::<Vec<_>>()
            .join(", ")
    );

    // The capture's settings: the words the rule reads were measured under the Windows locale
    // en-ZA, whose currency symbol is R.
    let setting = |name: &str| {
        environment
            .settings
            .iter()
            .find(|(setting, _)| setting == name)
            .map(|(_, value)| value.as_str())
    };
    for (name, value) in CAPTURED_SETTINGS {
        assert_eq!(setting(name), Some(value), "the capture's setting {name}");
    }
    note!(
        "ACCEPT-NOTE: {CASE}: Excel comparison through calc_verdict::judge against Excel {}'s capture of {LABEL_WORKBOOK_PATH} under {}, with COM add-ins connected {:?} ({:?}): {} (every computed row matched, every refused row not modeled with its owner); the whole workbook: {}",
        environment.target.excel_build(),
        CAPTURED_SETTINGS
            .iter()
            .map(|(name, value)| format!("{name} {value:?}"))
            .collect::<Vec<_>>()
            .join(", "),
        comparison.report.com_add_ins_connected,
        setting(calc_verdict::SETTING_COM_ADD_INS_CONNECTED),
        sheet_verdicts(&comparison.report, &rows),
        comparison.report.summary()
    );
}

// ---------------------------------------------------------------------------------------------
// dc-fwpz: the logical functions AND, OR, XOR, NOT, TRUE and FALSE through calc_func::invoke on
// catalogue targets.
//
// Each oracle row runs one call and asserts a value written here by hand, and says how it was
// obtained. A `documented` row applies a rule that Microsoft's page for the function states, named
// in the row. A `measured` row applies a rule that no page settles, and its value is Excel's raw
// value for the row's cell in the committed capture of the oracle workbook: how direct numbers,
// text, omitted arguments and a blank cell's value given directly (through IFNA or IFERROR, which
// return it as it is) read, the order in which errors win, and how NOT reads text and cells. A
// refused row ends in a fault and names the bead that owns the behaviour it refuses, never dc-fwpz
// itself. Every row is also run under NumericMode::Exact, with the same outcome: none of these
// functions rounds.
//
// `tests/fixtures/dc_fwpz/oracle.xlsx`, written by the committed generator
// `tests/fixtures/make-fixtures.ps1`, holds every row's formula in column D of its sheet, in the
// order the case runs its rows, beside the cells the rows read; dc_fwpz__logical_functions checks
// that the sheet holds exactly those formulas and cells. `cargo xtask capture --fixture
// crates/calc-func/tests/fixtures/dc_fwpz/oracle.xlsx --case dc_fwpz__excel_comparison --out
// crates/calc-func/tests/captures/dc_fwpz/oracle.capture.json` wrote Excel's capture of that
// workbook, and dc_fwpz__excel_comparison publishes the rows' results and their recorded invoke
// calls to calc_verdict::judge against it: each computed row must be matched, and each refused
// row not modelled with its owner.

/// The oracle workbook of dc-fwpz, as committed.
const LOGICAL_WORKBOOK: &[u8] = include_bytes!("fixtures/dc_fwpz/oracle.xlsx");
const LOGICAL_WORKBOOK_PATH: &str = "crates/calc-func/tests/fixtures/dc_fwpz/oracle.xlsx";
/// Excel's capture of that workbook.
const LOGICAL_CAPTURE: &[u8] = include_bytes!("captures/dc_fwpz/oracle.capture.json");
/// The case the capture is recorded under.
const LOGICAL_CAPTURE_CASE: &str = "dc_fwpz__excel_comparison";

/// The entries dc-fwpz adds to family 5, beside IF (member 1): each spelling and the member number
/// of its FunctionId.
const LOGICAL_ENTRIES: [(&str, u32); 6] = [
    ("AND", 2),
    ("OR", 3),
    ("XOR", 4),
    ("NOT", 5),
    ("TRUE", 6),
    ("FALSE", 7),
];

/// Why a dc-fwpz row expects its outcome.
#[derive(Clone, Copy, Debug)]
enum Logic {
    /// This rule, which Microsoft's page for the function states.
    Documented(&'static str),
    /// This rule, which no page settles; the row's value is Excel's raw value for its cell in the
    /// committed capture.
    Measured(&'static str),
    /// A refusal as not modelled; the named bead, never this one, owns the refused behaviour.
    Owned(Owner),
}

/// The rows of dc_fwpz__logical_functions, with how many are documented and how many measured.
struct LogicalRows {
    rows: Rows,
    documented: usize,
    measured: usize,
}

/// Runs the named call `spelling` on `args` through calc_func::invoke under Fast with the test
/// sheet as the cell reader and asserts `expect`, that the eager call asked for each argument once
/// in order, that the same call under Exact has the same outcome, and that the outcome fits
/// `logic` (a value, or a refusal for an owned row). Then records the row with the next cell of
/// column D and prints it with how its outcome was obtained.
fn logical_row(
    lr: &mut LogicalRows,
    sheet: &mut TestSheet,
    spelling: &'static str,
    args: &[Arg],
    expect: Expect,
    logic: Logic,
) {
    let (formula, asked, result, tick) =
        call_and_assert(sheet, &NumericMode::Fast, named(spelling), args, &expect);
    let in_order: Vec<usize> = (0..args.len()).collect();
    assert_eq!(asked, in_order, "{formula}: the arguments asked for");
    let exact = exact_mode();
    let under_exact = invoke(
        surface(Notation::Call, spelling),
        &mut Recorded::new(args.iter().map(|a| a.value.clone()).collect()),
        &mut EvalServices::with_cells(&exact, sheet),
        &mut FunctionCallScratch::new(),
    );
    assert!(
        match (&under_exact, &result) {
            (Ok(e), Ok(f)) => identical(e, f),
            (Err(e), Err(f)) => e == f,
            _ => false,
        },
        "{formula} under Exact gives {under_exact:?}, under Fast {result:?}"
    );
    let owner = match logic {
        Logic::Owned(owner) => {
            assert!(
                result.is_err(),
                "{formula}: an owned row computed {result:?}"
            );
            Some(owner)
        }
        Logic::Documented(_) | Logic::Measured(_) => {
            assert!(
                result.is_ok(),
                "{formula}: a row of dc-fwpz computes a value"
            );
            None
        }
    };
    let rows = &mut lr.rows;
    let row = u32::try_from(rows.oracle.len() + 1).expect("a row number");
    if !rows.quiet {
        let reads: Vec<&str> = args
            .iter()
            .flat_map(|a| a.reads.iter().map(String::as_str))
            .collect();
        let cells = if reads.is_empty() {
            String::new()
        } else {
            format!(" with {}", reads.join(", "))
        };
        let shown = if formula.len() > 64 {
            format!("{}... ({} arguments)", &formula[..24], args.len())
        } else {
            formula.clone()
        };
        let label = match logic {
            Logic::Documented(rule) => format!(
                "documented: {rule}; the same under Exact; dc_fwpz__excel_comparison compares it with Excel's capture"
            ),
            Logic::Measured(rule) => format!(
                "measured: {rule}; the value is Excel's raw value for this cell in the committed capture; the same under Exact; dc_fwpz__excel_comparison compares it with Excel's capture"
            ),
            Logic::Owned(owner) => format!(
                "refused as not modelled, owned by {}: {}; the same under Exact; dc_fwpz__excel_comparison publishes it as not modelled with that owner",
                owner.bead, owner.behaviour
            ),
        };
        note!(
            "ACCEPT-NOTE: {}: oracle {}!{} {shown}{cells} -> {} ({label})",
            rows.case,
            rows.sheet,
            encode_a1(row, FORMULA_COLUMN),
            expect.shown()
        );
    }
    rows.calls.push(Call { row, tick });
    rows.oracle.push(OracleRow {
        row,
        formula,
        outcome: result,
        choice: None,
        owner,
        tick,
    });
    match logic {
        Logic::Documented(_) => lr.documented += 1,
        Logic::Measured(_) => lr.measured += 1,
        Logic::Owned(_) => {}
    }
}

/// Runs a row of dc-fwpz: `lg!(rows, sheet, "SPELLING", [args], expect, logic)`.
macro_rules! lg {
    ($rows:expr, $sheet:expr, $spelling:expr, [$($arg:expr),* $(,)?], $expect:expr, $logic:expr) => {{
        let args: Vec<Arg> = vec![$($arg),*];
        logical_row($rows, $sheet, $spelling, &args, $expect, $logic)
    }};
}

// Rules the documented rows apply, as the functions' pages state them.
const TRUE_RETURNS: &str =
    "TRUE() returns the logical value TRUE and takes no argument (the TRUE page)";
const FALSE_RETURNS: &str =
    "FALSE() returns the logical value FALSE and takes no argument (the FALSE page)";
const AND_ALL: &str = "AND returns TRUE if all its arguments evaluate to TRUE and FALSE if one or more evaluate to FALSE; an argument may be an array or a reference that holds logicals (the AND page)";
const OR_ANY: &str = "OR returns TRUE if any of its arguments evaluates to TRUE and FALSE if all of them evaluate to FALSE (the OR page)";
const XOR_ODD: &str = "the result of XOR is TRUE when the number of TRUE inputs is odd and FALSE when it is even (the XOR page)";
const NOT_REVERSES: &str =
    "if logical is FALSE, NOT returns TRUE; if logical is TRUE, NOT returns FALSE (the NOT page)";
const HELD_IGNORED: &str = "text and empty cells in an array or a reference argument are ignored (the AND, OR and XOR pages)";
const NO_LOGICAL: &str =
    "a range that holds no logical value gives #VALUE! (the AND, OR and XOR pages)";
const MOST_255: &str = "AND and OR test up to a maximum of 255 conditions (their pages)";
const MOST_254: &str = "XOR tests 1 to 254 conditions (the XOR page)";

// Rules the measured rows apply, which no page states.
const NUMBER_LOGICAL: &str =
    "a number is TRUE unless it is 0, given directly, in a cell or in an array";
const DIRECT_SPELLING: &str =
    "direct text that spells TRUE or FALSE, in any ASCII case, is that logical";
const DIRECT_SKIPPED: &str = "any other direct text is skipped: a word, a digit, empty text, an error's spelling, a spelling with a space or in fullwidth letters";
const ONLY_SKIPPED: &str =
    "with nothing but skipped text no logical is read, and the result is #VALUE!";
const OMITTED_FALSE: &str = "an omitted argument is a FALSE";
const ALL_READ: &str =
    "every argument is read: an error after the value that decides the result is the result";
const ERROR_RESULT: &str = "an error among the arguments is the result";
const DIRECT_ERROR_ORDER: &str = "of the errors given directly, the first in argument order wins";
const DIRECT_FIRST: &str = "an error given directly wins over any error a reference or an array holds, wherever it stands among the arguments";
const HELD_ORDER: &str = "references and arrays are read in argument order, each range and each array row by row, and the first error met wins";
const NOT_TEXT: &str =
    "NOT reads text as IF reads its condition: TRUE or FALSE in any ASCII case, other text #VALUE!";
const NOT_CELL: &str =
    "a cell given to NOT is read as its value: a blank cell as FALSE, text as NOT reads text";
const BLANK_DIRECT: &str = "a blank cell's value given directly (IFNA and IFERROR return it as it is) is a FALSE, as an omitted argument is";
const NOT_BLANK_DIRECT: &str = "NOT reads a blank cell's value given directly (through IFNA or IFERROR) as FALSE, as it reads a blank cell";

/// NOT applied to an array lifts over its elements, which dc-m3u owns. Excel loads the oracle
/// workbook's NOT({TRUE,FALSE}) as the legacy formula =NOT(@{TRUE,FALSE}).
const LEGACY_ARRAY_NOT: Owner = Owner {
    bead: "dc-10y",
    behaviour: "an array argument of NOT in a legacy formula, which Excel loads as =NOT(@{TRUE,FALSE})",
};

/// The cells the logical rows read. A1:A10 hold one value of each kind AND, OR and XOR tell apart
/// (A7 is blank, A8 holds empty text); B1:B4 two errors and two logicals; A12:B13 a number, a
/// logical and two errors, one in each corner off the diagonal, so the error a range gives tells
/// row-by-row reading from column-by-column; C1:C4 text and a blank cell, no logical, and
/// C1048576 TRUE, the only logical of column C.
fn logical_functions_sheet() -> TestSheet {
    TestSheet::new(&[
        ("A1", "TRUE", CalcValue::logical(true)),
        ("A2", "FALSE", CalcValue::logical(false)),
        ("A3", "1", CalcValue::number(1.0)),
        ("A4", "0", CalcValue::number(0.0)),
        ("A5", "text \"TRUE\"", text_value("TRUE")),
        ("A6", "text \"x\"", text_value("x")),
        ("A8", "empty text", text_value("")),
        ("A9", "-2.5", CalcValue::number(-2.5)),
        ("A10", "text \"FALSE\"", text_value("FALSE")),
        ("A12", "1", CalcValue::number(1.0)),
        ("A13", "#REF!", CalcValue::error(WorksheetErrorCode::Ref)),
        ("B1", "#DIV/0!", CalcValue::error(WorksheetErrorCode::Div0)),
        ("B2", "#N/A", CalcValue::error(WorksheetErrorCode::NA)),
        ("B3", "TRUE", CalcValue::logical(true)),
        ("B4", "FALSE", CalcValue::logical(false)),
        ("B12", "#NUM!", CalcValue::error(WorksheetErrorCode::Num)),
        ("B13", "TRUE", CalcValue::logical(true)),
        ("C1", "text \"abc\"", text_value("abc")),
        ("C3", "text \"TRUE\"", text_value("TRUE")),
        ("C4", "text \"1\"", text_value("1")),
        ("C1048576", "TRUE", CalcValue::logical(true)),
    ])
}

/// The rows of dc_fwpz__logical_functions, run through calc_func::invoke on `s`, in the order the
/// oracle workbook's sheet `logical_functions` holds their formulas.
fn logical_functions_rows(s: &mut TestSheet, quiet: bool) -> LogicalRows {
    const CASE: &str = "dc_fwpz__logical_functions";
    let mut lr = LogicalRows {
        rows: Rows::new(CASE, "logical_functions", quiet),
        documented: 0,
        measured: 0,
    };
    let r = &mut lr;
    use Logic::{Documented as Doc, Measured as Meas, Owned};
    use WorksheetErrorCode::{Div0, NA, Num, Value};
    let t_ = || logical(true);
    let f_ = || logical(false);
    let yes = || CalcValue::logical(true);
    let no = || CalcValue::logical(false);
    let error = CalcValue::error;

    lg!(r, s, "TRUE", [], L(true), Doc(TRUE_RETURNS));
    lg!(r, s, "FALSE", [], L(false), Doc(FALSE_RETURNS));

    // AND: direct arguments.
    lg!(r, s, "AND", [t_(), t_()], L(true), Doc(AND_ALL));
    lg!(r, s, "AND", [t_(), f_()], L(false), Doc(AND_ALL));
    lg!(
        r,
        s,
        "AND",
        [num("1"), num("2")],
        L(true),
        Meas(NUMBER_LOGICAL)
    );
    lg!(
        r,
        s,
        "AND",
        [num("1"), num("0")],
        L(false),
        Meas(NUMBER_LOGICAL)
    );
    lg!(r, s, "AND", [num("-2.5")], L(true), Meas(NUMBER_LOGICAL));
    lg!(
        r,
        s,
        "AND",
        [txt("TRUE"), t_()],
        L(true),
        Meas(DIRECT_SPELLING)
    );
    lg!(
        r,
        s,
        "AND",
        [txt("false"), t_()],
        L(false),
        Meas(DIRECT_SPELLING)
    );
    lg!(r, s, "AND", [t_(), txt("x")], L(true), Meas(DIRECT_SKIPPED));
    lg!(r, s, "AND", [txt("x")], E(Value), Meas(ONLY_SKIPPED));
    lg!(r, s, "AND", [t_(), txt("1")], L(true), Meas(DIRECT_SKIPPED));
    lg!(r, s, "AND", [t_(), txt("0")], L(true), Meas(DIRECT_SKIPPED));
    lg!(r, s, "AND", [t_(), txt("")], L(true), Meas(DIRECT_SKIPPED));
    lg!(
        r,
        s,
        "AND",
        [t_(), omitted()],
        L(false),
        Meas(OMITTED_FALSE)
    );
    lg!(
        r,
        s,
        "AND",
        [omitted(), omitted()],
        L(false),
        Meas(OMITTED_FALSE)
    );
    lg!(r, s, "AND", [f_(), err(NA)], E(NA), Meas(ALL_READ));
    lg!(
        r,
        s,
        "AND",
        [err(NA), err(Div0)],
        E(NA),
        Meas(DIRECT_ERROR_ORDER)
    );
    lg!(
        r,
        s,
        "AND",
        [err(Div0), err(NA)],
        E(Div0),
        Meas(DIRECT_ERROR_ORDER)
    );
    lg!(r, s, "AND", [txt("x"), err(NA)], E(NA), Meas(ERROR_RESULT));

    // AND: references and arrays.
    lg!(
        r,
        s,
        "AND",
        [reference(s, &["A1:A10"])],
        L(false),
        Doc(HELD_IGNORED)
    );
    lg!(
        r,
        s,
        "AND",
        [reference(s, &["A3"]), reference(s, &["A9"])],
        L(true),
        Meas(NUMBER_LOGICAL)
    );
    lg!(
        r,
        s,
        "AND",
        [reference(s, &["A5:A8"])],
        E(Value),
        Doc(NO_LOGICAL)
    );
    lg!(
        r,
        s,
        "AND",
        [reference(s, &["C1:C4"])],
        E(Value),
        Doc(NO_LOGICAL)
    );
    lg!(
        r,
        s,
        "AND",
        [reference(s, &["C:C"])],
        L(true),
        Doc(HELD_IGNORED)
    );
    lg!(
        r,
        s,
        "AND",
        [reference(s, &["A1"]), reference(s, &["A5"])],
        L(true),
        Doc(HELD_IGNORED)
    );
    lg!(
        r,
        s,
        "AND",
        [reference(s, &["A10"]), t_()],
        L(true),
        Doc(HELD_IGNORED)
    );
    lg!(
        r,
        s,
        "AND",
        [array_arg("{TRUE,FALSE}", vec![vec![yes(), no()]])],
        L(false),
        Doc(AND_ALL)
    );
    lg!(
        r,
        s,
        "AND",
        [array_arg(
            "{1,2,\"x\"}",
            vec![vec![
                CalcValue::number(1.0),
                CalcValue::number(2.0),
                text_value("x")
            ]]
        )],
        L(true),
        Meas(NUMBER_LOGICAL)
    );
    lg!(
        r,
        s,
        "AND",
        [array_arg("{\"TRUE\"}", vec![vec![text_value("TRUE")]])],
        E(Value),
        Doc(HELD_IGNORED)
    );
    lg!(
        r,
        s,
        "AND",
        [reference(s, &["B1:B4"])],
        E(Div0),
        Meas(HELD_ORDER)
    );
    lg!(
        r,
        s,
        "AND",
        [reference(s, &["B3:B4"]), reference(s, &["B2"])],
        E(NA),
        Meas(ALL_READ)
    );
    lg!(
        r,
        s,
        "AND",
        [reference(s, &["B2"]), err(Div0)],
        E(Div0),
        Meas(DIRECT_FIRST)
    );
    lg!(
        r,
        s,
        "AND",
        [err(Div0), reference(s, &["B2"])],
        E(Div0),
        Meas(DIRECT_FIRST)
    );
    lg!(
        r,
        s,
        "AND",
        [
            array_arg("{TRUE,#N/A}", vec![vec![yes(), error(NA)]]),
            err(Div0)
        ],
        E(Div0),
        Meas(DIRECT_FIRST)
    );
    lg!(
        r,
        s,
        "AND",
        [
            array_arg("{TRUE,#DIV/0!}", vec![vec![yes(), error(Div0)]]),
            reference(s, &["B2"])
        ],
        E(Div0),
        Meas(HELD_ORDER)
    );
    lg!(
        r,
        s,
        "AND",
        [
            reference(s, &["B2"]),
            array_arg("{TRUE,#DIV/0!}", vec![vec![yes(), error(Div0)]])
        ],
        E(NA),
        Meas(HELD_ORDER)
    );
    lg!(
        r,
        s,
        "AND",
        [reference(s, &["B2"]), txt("x"), err(Div0)],
        E(Div0),
        Meas(DIRECT_FIRST)
    );
    lg!(
        r,
        s,
        "AND",
        [reference(s, &["A1:A2", "B3:B4"])],
        L(false),
        Doc(AND_ALL)
    );
    lg!(
        r,
        s,
        "AND",
        [array_arg(
            "{TRUE,#N/A,#DIV/0!}",
            vec![vec![yes(), error(NA), error(Div0)]]
        )],
        E(NA),
        Meas(HELD_ORDER)
    );
    lg!(
        r,
        s,
        "AND",
        [array_arg(
            "{TRUE,#N/A;#DIV/0!,TRUE}",
            vec![vec![yes(), error(NA)], vec![error(Div0), yes()]]
        )],
        E(NA),
        Meas(HELD_ORDER)
    );
    lg!(
        r,
        s,
        "AND",
        [f_(), array_arg("{TRUE,#N/A}", vec![vec![yes(), error(NA)]])],
        E(NA),
        Meas(ALL_READ)
    );
    lg!(
        r,
        s,
        "AND",
        [reference(s, &["A7"])],
        E(Value),
        Doc(NO_LOGICAL)
    );
    let many_true: Vec<Arg> = (0..255).map(|_| t_()).collect();
    logical_row(r, s, "AND", &many_true, L(true), Doc(MOST_255));

    // OR.
    lg!(r, s, "OR", [f_(), f_()], L(false), Doc(OR_ANY));
    lg!(r, s, "OR", [f_(), t_()], L(true), Doc(OR_ANY));
    lg!(
        r,
        s,
        "OR",
        [num("0"), num("0")],
        L(false),
        Meas(NUMBER_LOGICAL)
    );
    lg!(
        r,
        s,
        "OR",
        [num("0"), num("-2.5")],
        L(true),
        Meas(NUMBER_LOGICAL)
    );
    lg!(
        r,
        s,
        "OR",
        [f_(), txt("TRUE")],
        L(true),
        Meas(DIRECT_SPELLING)
    );
    lg!(r, s, "OR", [f_(), txt("x")], L(false), Meas(DIRECT_SKIPPED));
    lg!(r, s, "OR", [f_(), txt("1")], L(false), Meas(DIRECT_SKIPPED));
    lg!(r, s, "OR", [f_(), txt("")], L(false), Meas(DIRECT_SKIPPED));
    lg!(
        r,
        s,
        "OR",
        [f_(), txt(" TRUE")],
        L(false),
        Meas(DIRECT_SKIPPED)
    );
    lg!(
        r,
        s,
        "OR",
        [f_(), txt("\u{FF34}\u{FF32}\u{FF35}\u{FF25}")],
        L(false),
        Meas(DIRECT_SKIPPED)
    );
    lg!(
        r,
        s,
        "OR",
        [f_(), txt("#N/A")],
        L(false),
        Meas(DIRECT_SKIPPED)
    );
    lg!(
        r,
        s,
        "OR",
        [txt("x"), txt("y")],
        E(Value),
        Meas(ONLY_SKIPPED)
    );
    lg!(r, s, "OR", [f_(), omitted()], L(false), Meas(OMITTED_FALSE));
    lg!(
        r,
        s,
        "OR",
        [omitted(), omitted()],
        L(false),
        Meas(OMITTED_FALSE)
    );
    lg!(r, s, "OR", [t_(), err(NA)], E(NA), Meas(ALL_READ));
    lg!(
        r,
        s,
        "OR",
        [reference(s, &["B1"]), err(NA)],
        E(NA),
        Meas(DIRECT_FIRST)
    );
    lg!(
        r,
        s,
        "OR",
        [reference(s, &["A1:A10"])],
        L(true),
        Doc(HELD_IGNORED)
    );
    lg!(
        r,
        s,
        "OR",
        [reference(s, &["A2"]), reference(s, &["A4"])],
        L(false),
        Meas(NUMBER_LOGICAL)
    );
    lg!(
        r,
        s,
        "OR",
        [reference(s, &["A5:A8"])],
        E(Value),
        Doc(NO_LOGICAL)
    );
    lg!(
        r,
        s,
        "OR",
        [reference(s, &["C:C"])],
        L(true),
        Doc(HELD_IGNORED)
    );
    lg!(
        r,
        s,
        "OR",
        [array_arg(
            "{FALSE,0,\"TRUE\"}",
            vec![vec![no(), CalcValue::number(0.0), text_value("TRUE")]]
        )],
        L(false),
        Meas(NUMBER_LOGICAL)
    );
    lg!(
        r,
        s,
        "OR",
        [reference(s, &["B3:B4"]), reference(s, &["B1"])],
        E(Div0),
        Meas(ALL_READ)
    );
    lg!(
        r,
        s,
        "OR",
        [reference(s, &["A6"]), f_()],
        L(false),
        Doc(HELD_IGNORED)
    );
    lg!(
        r,
        s,
        "OR",
        [reference(s, &["A12:B13"])],
        E(Num),
        Meas(HELD_ORDER)
    );
    let many_false: Vec<Arg> = (0..255).map(|_| f_()).collect();
    logical_row(r, s, "OR", &many_false, L(false), Doc(MOST_255));

    // XOR.
    lg!(r, s, "XOR", [t_(), f_()], L(true), Doc(XOR_ODD));
    lg!(r, s, "XOR", [t_(), t_()], L(false), Doc(XOR_ODD));
    lg!(r, s, "XOR", [t_(), t_(), t_()], L(true), Doc(XOR_ODD));
    lg!(r, s, "XOR", [f_(), f_()], L(false), Doc(XOR_ODD));
    lg!(
        r,
        s,
        "XOR",
        [num("1"), num("2")],
        L(false),
        Meas(NUMBER_LOGICAL)
    );
    lg!(
        r,
        s,
        "XOR",
        [reference(s, &["A1:A10"])],
        L(true),
        Meas(NUMBER_LOGICAL)
    );
    lg!(
        r,
        s,
        "XOR",
        [reference(s, &["A5:A8"])],
        E(Value),
        Doc(NO_LOGICAL)
    );
    lg!(
        r,
        s,
        "XOR",
        [array_arg("{TRUE,TRUE}", vec![vec![yes(), yes()]]), t_()],
        L(true),
        Doc(XOR_ODD)
    );
    lg!(r, s, "XOR", [t_(), txt("x")], L(true), Meas(DIRECT_SKIPPED));
    lg!(r, s, "XOR", [txt("x")], E(Value), Meas(ONLY_SKIPPED));
    lg!(r, s, "XOR", [t_(), err(NA)], E(NA), Meas(ERROR_RESULT));
    lg!(
        r,
        s,
        "XOR",
        [reference(s, &["B1"]), reference(s, &["B2"])],
        E(Div0),
        Meas(HELD_ORDER)
    );
    lg!(
        r,
        s,
        "XOR",
        [reference(s, &["B2"]), err(Div0)],
        E(Div0),
        Meas(DIRECT_FIRST)
    );
    lg!(
        r,
        s,
        "XOR",
        [reference(s, &["C:C"])],
        L(true),
        Doc(HELD_IGNORED)
    );
    lg!(r, s, "XOR", [t_(), omitted()], L(true), Meas(OMITTED_FALSE));
    logical_row(r, s, "XOR", &many_true[..254], L(false), Doc(MOST_254));

    // NOT.
    lg!(r, s, "NOT", [t_()], L(false), Doc(NOT_REVERSES));
    lg!(r, s, "NOT", [f_()], L(true), Doc(NOT_REVERSES));
    lg!(r, s, "NOT", [num("0")], L(true), Meas(NUMBER_LOGICAL));
    lg!(r, s, "NOT", [num("-2.5")], L(false), Meas(NUMBER_LOGICAL));
    lg!(r, s, "NOT", [txt("TRUE")], L(false), Meas(NOT_TEXT));
    lg!(r, s, "NOT", [txt("false")], L(true), Meas(NOT_TEXT));
    lg!(r, s, "NOT", [txt("x")], E(Value), Meas(NOT_TEXT));
    lg!(r, s, "NOT", [txt("")], E(Value), Meas(NOT_TEXT));
    lg!(r, s, "NOT", [err(NA)], E(NA), Meas(ERROR_RESULT));
    lg!(r, s, "NOT", [cell(s, "A7")], L(true), Meas(NOT_CELL));
    lg!(r, s, "NOT", [cell(s, "A5")], L(false), Meas(NOT_CELL));
    lg!(r, s, "NOT", [cell(s, "A6")], E(Value), Meas(NOT_CELL));
    lg!(r, s, "NOT", [cell(s, "A8")], E(Value), Meas(NOT_CELL));
    lg!(r, s, "NOT", [cell(s, "B1")], E(Div0), Meas(ERROR_RESULT));
    lg!(r, s, "NOT", [cell(s, "A3")], L(false), Meas(NUMBER_LOGICAL));
    lg!(
        r,
        s,
        "NOT",
        [array_arg("{TRUE,FALSE}", vec![vec![yes(), no()]])],
        Refused(Some(0)),
        Owned(LEGACY_ARRAY_NOT)
    );
    lg!(r, s, "NOT", [txt("1")], E(Value), Meas(NOT_TEXT));

    // Digit text alone, and two arrays.
    lg!(r, s, "AND", [txt("1")], E(Value), Meas(ONLY_SKIPPED));
    lg!(
        r,
        s,
        "AND",
        [
            array_arg("{FALSE,TRUE,TRUE}", vec![vec![no(), yes(), yes()]]),
            array_arg("{TRUE,TRUE,TRUE}", vec![vec![yes(), yes(), yes()]])
        ],
        L(false),
        Doc(AND_ALL)
    );

    // A7's blank value given directly: IFNA and IFERROR, each run first through calc_func::invoke,
    // return it as it is, the empty value, and the outer call receives that value.
    let ifna_blank = |s: &mut TestSheet| nest!(s, "IFNA", [cell(s, "A7"), num("0")]);
    let iferror_blank = |s: &mut TestSheet| nest!(s, "IFERROR", [cell(s, "A7"), f_()]);
    for blank in [ifna_blank(s), iferror_blank(s)] {
        assert!(
            matches!(blank.value.core(), CoreValue::Empty),
            "{} gives {:?}",
            blank.excel,
            blank.value
        );
    }
    lg!(
        r,
        s,
        "AND",
        [t_(), ifna_blank(s)],
        L(false),
        Meas(BLANK_DIRECT)
    );
    lg!(r, s, "AND", [ifna_blank(s)], L(false), Meas(BLANK_DIRECT));
    lg!(
        r,
        s,
        "AND",
        [t_(), iferror_blank(s)],
        L(false),
        Meas(BLANK_DIRECT)
    );
    lg!(
        r,
        s,
        "OR",
        [f_(), iferror_blank(s)],
        L(false),
        Meas(BLANK_DIRECT)
    );
    lg!(r, s, "OR", [iferror_blank(s)], L(false), Meas(BLANK_DIRECT));
    lg!(r, s, "OR", [ifna_blank(s)], L(false), Meas(BLANK_DIRECT));
    lg!(
        r,
        s,
        "XOR",
        [t_(), ifna_blank(s)],
        L(true),
        Meas(BLANK_DIRECT)
    );
    lg!(r, s, "XOR", [ifna_blank(s)], L(false), Meas(BLANK_DIRECT));
    lg!(
        r,
        s,
        "XOR",
        [iferror_blank(s)],
        L(false),
        Meas(BLANK_DIRECT)
    );
    lg!(
        r,
        s,
        "NOT",
        [ifna_blank(s)],
        L(true),
        Meas(NOT_BLANK_DIRECT)
    );
    lg!(
        r,
        s,
        "NOT",
        [iferror_blank(s)],
        L(true),
        Meas(NOT_BLANK_DIRECT)
    );
    lr
}

/// One call of `spelling` on `values` through calc_func::invoke with `cells` as the reader.
fn logical_call(
    spelling: &str,
    values: Vec<CalcValue>,
    mode: &NumericMode,
    cells: &mut dyn CellReader,
) -> Result<CalcValue, FunctionFault> {
    invoke(
        surface(Notation::Call, spelling),
        &mut Recorded::new(values),
        &mut EvalServices::with_cells(mode, cells),
        &mut FunctionCallScratch::new(),
    )
}

#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test)]
fn dc_fwpz__logical_functions() {
    const CASE: &str = "dc_fwpz__logical_functions";
    let fast = NumericMode::Fast;
    let mut sheet = logical_functions_sheet();
    note!(
        "ACCEPT-NOTE: {CASE}: the sheet holds {}; A7 and C2 are blank",
        sheet.described()
    );
    let lr = logical_functions_rows(&mut sheet, false);
    let rows = &lr.rows;
    let s = &mut sheet;

    // Metadata and dispatch come from the one generated catalogue: family 5 holds IF and exactly
    // these entries, each eager, with Excel's arity, its parameter classes and its signature help.
    let family: BTreeSet<(u32, String)> = catalogue()
        .targets()
        .filter(|target| target.id().raw() >> 16 == 5)
        .map(|target| (target.id().raw(), target.spelling().to_owned()))
        .collect();
    let mut expected: BTreeSet<(u32, String)> = LOGICAL_ENTRIES
        .iter()
        .map(|(spelling, member)| ((5 << 16) | member, (*spelling).to_owned()))
        .collect();
    expected.insert(((5 << 16) | 1, "IF".to_owned()));
    assert_eq!(family, expected, "the logical family in the catalogue");
    let either = Some(ParameterClass::ValueOrReference);
    let mut outside_arity = 0;
    for (spelling, arity, class, help) in [
        ("AND", (1, 255), either, "AND(logical1, [logical2], ...)"),
        ("OR", (1, 255), either, "OR(logical1, [logical2], ...)"),
        ("XOR", (1, 254), either, "XOR(logical1, [logical2], ...)"),
        ("NOT", (1, 1), Some(ParameterClass::Value), "NOT(logical)"),
        ("TRUE", (0, 0), None, "TRUE()"),
        ("FALSE", (0, 0), None, "FALSE()"),
    ] {
        let target = surface(Notation::Call, spelling);
        assert_eq!(
            target.argument_demand(),
            ArgumentDemand::Eager,
            "{spelling}"
        );
        let shape = target.arity();
        assert_eq!((shape.min, shape.max), arity, "{spelling}");
        for index in 0..shape.max {
            assert_eq!(target.parameter_class(index), class, "{spelling} {index}");
        }
        assert_eq!(target.parameter_class(shape.max), None, "{spelling}");
        assert_eq!(target.signature_help().display, help);
        // Outside the arity the call is refused before any argument is asked for.
        for count in std::iter::once(shape.max + 1).chain(shape.min.checked_sub(1)) {
            let mut given = Recorded::numbers(count);
            assert_eq!(
                run(target, &mut given, &fast),
                Err(FunctionFault::ArityMismatch {
                    id: target.id(),
                    supplied: count
                }),
                "{spelling} with {count} arguments"
            );
            assert!(given.asked.is_empty(), "{spelling} asked {:?}", given.asked);
            outside_arity += 1;
        }
    }
    note!(
        "ACCEPT-NOTE: {CASE}: the catalogue's family 5 holds IF and exactly {:?}, each eager; AND and OR take 1 to 255 values or references, XOR 1 to 254, NOT one value, TRUE and FALSE none; {outside_arity} calls outside an entry's arity ended in ArityMismatch with no argument asked for",
        LOGICAL_ENTRIES.map(|(spelling, _)| spelling)
    );

    // Through the call interface alone. What is no worksheet value, or what another bead owns, is
    // refused naming the argument: a number that is no worksheet value (NaN and an infinity, and a
    // subnormal, which dc-6qki owns), directly or in an array; an array or a reference inside
    // an array (array evaluation, dc-m3u); and a callable, directly or in an array, whose core
    // is #CALC! but which is no worksheet error until it reaches a cell (LAMBDA, dc-m3u). No
    // oracle row measures these. NOT's parameter takes a value, so a reference is refused at
    // admission.
    let lambda = CalcValue::callable(CallableValue::new(
        CallableArityShape::exact(1),
        "LAMBDA(x, x)",
        Arc::new(Token(5)),
    ));
    let held_lambda = CalcValue::array(
        CalcArray::from_rows(vec![vec![CalcValue::logical(true), lambda.clone()]])
            .expect("a 1-by-2 array"),
    );
    let nested = CalcValue::array(
        CalcArray::from_rows(vec![vec![CalcValue::array(CalcArray::from_scalar(
            CalcValue::logical(true),
        ))]])
        .expect("a 1-by-1 array"),
    );
    let reference_held = CalcValue::array(
        CalcArray::from_rows(vec![vec![reference(s, &["A1"]).value]]).expect("a 1-by-1 array"),
    );
    let held_nan = CalcValue::array(CalcArray::from_scalar(CalcValue::number(f64::NAN)));
    let mut refused_calls = 0;
    for (spelling, values, index) in [
        ("AND", vec![CalcValue::logical(true), lambda.clone()], 1),
        ("OR", vec![lambda.clone()], 0),
        ("XOR", vec![CalcValue::logical(true), lambda.clone()], 1),
        ("NOT", vec![lambda.clone()], 0),
        ("AND", vec![held_lambda.clone()], 0),
        ("XOR", vec![CalcValue::number(f64::NAN)], 0),
        ("AND", vec![CalcValue::number(f64::INFINITY)], 0),
        ("OR", vec![CalcValue::number(f64::from_bits(1))], 0),
        ("XOR", vec![CalcValue::logical(true), held_nan.clone()], 1),
        ("AND", vec![nested.clone()], 0),
        ("OR", vec![reference_held.clone()], 0),
        ("NOT", vec![CalcValue::number(f64::NAN)], 0),
    ] {
        let id = surface(Notation::Call, spelling).id();
        assert_eq!(
            logical_call(spelling, values.clone(), &fast, s),
            Err(FunctionFault::NotImplemented {
                id,
                argument: Some(index)
            }),
            "{spelling} on {values:?}"
        );
        refused_calls += 1;
    }
    let not = surface(Notation::Call, "NOT");
    assert_eq!(
        logical_call("NOT", vec![reference(s, &["A1"]).value], &fast, s),
        Err(FunctionFault::ReferenceArgument {
            id: not.id(),
            index: 0
        })
    );
    // A reference is read only through the services' reader: without one, or over one that
    // refuses, the call ends in ReferenceUnreadable naming it. A direct error is the result
    // before any reference is read, so it needs no reader.
    let mut unreadable = 0;
    for spelling in ["AND", "OR", "XOR"] {
        let target = surface(Notation::Call, spelling);
        let values = vec![CalcValue::logical(true), reference(s, &["A1:A10"]).value];
        let fault = Err(FunctionFault::ReferenceUnreadable {
            id: target.id(),
            index: 1,
        });
        assert_eq!(
            run(target, &mut Recorded::new(values.clone()), &fast),
            fault,
            "{spelling} without a cell reader"
        );
        s.refuse = true;
        let refused = logical_call(spelling, values, &fast, s);
        s.refuse = false;
        assert_eq!(refused, fault, "{spelling} over a refusing reader");
        unreadable += 2;
        let direct_error = vec![
            reference(s, &["A1:A10"]).value,
            CalcValue::error(WorksheetErrorCode::NA),
        ];
        assert_eq!(
            run(target, &mut Recorded::new(direct_error), &fast),
            Ok(CalcValue::error(WorksheetErrorCode::NA)),
            "{spelling} with a direct error and no cell reader"
        );
    }
    note!(
        "ACCEPT-NOTE: {CASE}: through the call interface alone: {refused_calls} calls refused a callable (directly to AND, OR, XOR and NOT, and held in an array), NaN, an infinity or a subnormal (directly or in an array), and an array or a reference inside an array (NotImplemented naming the argument); NOT refused a reference at admission (ReferenceArgument); {unreadable} calls of AND, OR and XOR without a reader or over a refusing one ended in ReferenceUnreadable naming the reference, and each gave a direct #N/A without reading its reference"
    );

    // A whole column costs what the reader visits: AND(C:C) asks for column C once and is given
    // its four held cells.
    s.reads.clear();
    s.visited = 0;
    let column = logical_call("AND", vec![reference(s, &["C:C"]).value], &fast, s);
    assert_eq!(column, Ok(CalcValue::logical(true)));
    assert_eq!((s.reads.clone(), s.visited), (vec![s.area("C:C")], 4));
    // A reader may also visit a blank cell, as Empty: over such a reader each call that reads a
    // bounded range gives the value of its oracle row.
    let mut dense_calls = 0;
    for (spelling, areas) in [
        ("AND", vec!["A1:A10"]),
        ("AND", vec!["A5:A8"]),
        ("AND", vec!["C1:C4"]),
        ("AND", vec!["A7"]),
        ("AND", vec!["A1:A2", "B3:B4"]),
        ("OR", vec!["A5:A8"]),
        ("OR", vec!["A12:B13"]),
        ("XOR", vec!["A1:A10"]),
    ] {
        let target = surface(Notation::Call, spelling);
        let argument = reference(s, &areas);
        let formula = target
            .oracle_formula(&[argument.excel.as_str()])
            .expect("a formula");
        let mut dense = DenseReader(s);
        let visited_blanks = logical_call(spelling, vec![argument.value], &fast, &mut dense);
        let under_fast = row_value(rows, &formula);
        assert!(
            visited_blanks
                .as_ref()
                .is_ok_and(|value| identical(value, under_fast)),
            "{formula} over a reader that visits blank cells gives {visited_blanks:?}, the row {under_fast:?}"
        );
        dense_calls += 1;
    }
    // A reader that visits a cell outside the area it was asked for cannot supply the range.
    let range = reference(s, &["A1:A2"]).value;
    let and = surface(Notation::Call, "AND");
    let mut breaching = BreachingReader {
        sheet: s,
        breach: Breach::Outside("C1"),
    };
    assert_eq!(
        logical_call("AND", vec![range], &fast, &mut breaching),
        Err(FunctionFault::ReferenceUnreadable {
            id: and.id(),
            index: 0
        }),
        "AND(A1:A2) over a reader that also visits C1"
    );
    note!(
        "ACCEPT-NOTE: {CASE}: AND(C:C) asked the reader for column C once and was given its 4 held cells; {dense_calls} calls over a reader that also visits every blank cell as Empty gave the values of their oracle rows; AND(A1:A2) over a reader that visits C1 outside the area ended in ReferenceUnreadable (argument 0)"
    );

    // The oracle workbook: its sheet holds exactly the rows' formulas and cells (XOR and IFNA
    // stored with the `_xlfn.` prefix, matched as whole names), and the committed workbook is its
    // generator's output.
    assert_eq!(
        stored_formula("BITXOR(1,XOR(TRUE),IFNA(1,2),_xlfn.XOR(TRUE))"),
        "BITXOR(1,_xlfn.XOR(TRUE),_xlfn.IFNA(1,2),_xlfn.XOR(TRUE))"
    );
    assert_workbook_sheet(LOGICAL_WORKBOOK, LOGICAL_WORKBOOK_PATH, rows, s);
    #[cfg(all(windows, not(target_arch = "wasm32")))]
    {
        let generated = generated_fixtures();
        let names: Vec<&str> = generated.keys().map(String::as_str).collect();
        assert_eq!(
            names, GENERATED_WORKBOOKS,
            "the generator writes the oracle workbooks and nothing else"
        );
        let regenerated = &generated["dc_fwpz/oracle.xlsx"];
        assert!(
            regenerated.as_slice() == LOGICAL_WORKBOOK,
            "the committed dc-fwpz oracle workbook is not the generator's output (sha256 {} committed, {} generated)",
            sha256_hex(LOGICAL_WORKBOOK),
            sha256_hex(regenerated)
        );
        note!(
            "ACCEPT-NOTE: {CASE}: crates/calc-func/tests/fixtures/make-fixtures.ps1, run now under Windows PowerShell, wrote the committed dc-fwpz oracle workbook byte for byte ({} bytes)",
            regenerated.len()
        );
    }
    #[cfg(not(all(windows, not(target_arch = "wasm32"))))]
    {
        note!(
            "ACCEPT-NOTE: {CASE}: the generator runs under Windows PowerShell, so the check that the committed oracle workbook is its output runs on the windows route; here the workbook is checked against the rows only"
        );
    }
    let mut owners: BTreeMap<&str, usize> = BTreeMap::new();
    for row in &rows.oracle {
        assert_eq!(
            row.owner.is_some(),
            row.outcome.is_err(),
            "{}: a refused row names its owner, a computed row none",
            row.formula
        );
        if let Some(owner) = row.owner {
            *owners.entry(owner.bead).or_default() += 1;
        }
    }
    assert_eq!(
        lr.documented + lr.measured + rows.refused(),
        rows.oracle.len()
    );
    let owners: Vec<String> = owners.iter().map(|(b, c)| format!("{c} {b}")).collect();
    note!(
        "ACCEPT-NOTE: {CASE}: {} rows through calc_func::invoke, each asserted against an outcome written here by hand and run again under Exact with the same outcome, each with its formula on sheet {} of {LOGICAL_WORKBOOK_PATH}: {} documented, {} measured (their values are Excel's raw values in the committed capture), {} refused (owned by: {}); dc_fwpz__excel_comparison compares every row with Excel's capture",
        rows.oracle.len(),
        rows.sheet,
        lr.documented,
        lr.measured,
        rows.refused(),
        owners.join(", ")
    );
}

#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test)]
fn dc_fwpz__excel_comparison() {
    const CASE: &str = "dc_fwpz__excel_comparison";
    let LogicalRows { rows, .. } = logical_functions_rows(&mut logical_functions_sheet(), true);
    let all = rows.oracle.len();
    let refused = rows.refused();
    let computed = all - refused;
    assert!(
        rows.oracle
            .iter()
            .all(|row| row.owner.is_some() == row.outcome.is_err()),
        "a refused row names its owner, a computed row none"
    );
    let comparison_of = |capture: &CaptureRecord, run: &Rows| {
        let (results, evaluations) = published_in(LOGICAL_WORKBOOK, LOGICAL_CAPTURE_CASE, &[run]);
        judged(capture, &results, &evaluations, run)
    };

    // The comparison itself, on capture records built here in memory, which carry no Excel
    // evidence: a capture that observes each computed row's own value matches every computed row
    // and counts every refused row not modelled with its owner; a capture that observes the other
    // logical for one computed row is exactly one problem, at that row's cell; a refused row that
    // names dc-fwpz as its owner is exactly one problem, at its cell.
    let agreeing_capture = capture_in(LOGICAL_WORKBOOK, LOGICAL_CAPTURE_CASE, &[&rows], agreeing);
    let comparison = comparison_of(&agreeing_capture, &rows)
        .unwrap_or_else(|refusal| panic!("judge refused the in-memory capture: {refusal}"));
    assert_eq!(comparison.problems, Vec::<String>::new());
    let counts = &comparison.report.counts;
    assert_eq!(
        (
            counts.denominator,
            counts.calculated,
            counts.matched,
            counts.not_modeled,
            counts.mismatched + counts.not_run + counts.unproven + counts.stale,
            counts.evaluator_calls
        ),
        (
            all as u64,
            computed as u64,
            computed as u64,
            refused as u64,
            0,
            all as u64
        ),
        "{}",
        comparison.report.summary()
    );
    let changed = rows
        .oracle
        .iter()
        .find(|row| {
            row.outcome
                .as_ref()
                .is_ok_and(|value| value.as_logical().is_some())
        })
        .expect("a computed row with a logical")
        .row;
    let other_logical = capture_in(
        LOGICAL_WORKBOOK,
        LOGICAL_CAPTURE_CASE,
        &[&rows],
        |sheet, row| {
            if row.row == changed {
                let logical = row.outcome.as_ref().ok()?.as_logical()?;
                Some(CalcValue::logical(!logical))
            } else {
                agreeing(sheet, row)
            }
        },
    );
    let problems = comparison_of(&other_logical, &rows)
        .unwrap_or_else(|refusal| panic!("judge refused the in-memory capture: {refusal}"))
        .problems;
    let cell = format!("{}!{}", rows.sheet, encode_a1(changed, FORMULA_COLUMN));
    assert!(
        problems.len() == 1 && problems[0].starts_with(&format!("{cell} ")),
        "a capture with the other logical at {cell} gives {problems:?}"
    );
    let LogicalRows {
        rows: mut self_owned,
        ..
    } = logical_functions_rows(&mut logical_functions_sheet(), true);
    let row = self_owned
        .oracle
        .iter_mut()
        .find(|row| row.outcome.is_err())
        .expect("a refused row");
    row.owner = Some(Owner {
        bead: "dc-fwpz",
        behaviour: "a refusal of the bead under test",
    });
    let self_cell = format!(
        "{}!{}",
        self_owned.sheet,
        encode_a1(row.row, FORMULA_COLUMN)
    );
    let problems = comparison_of(
        &capture_in(
            LOGICAL_WORKBOOK,
            LOGICAL_CAPTURE_CASE,
            &[&self_owned],
            agreeing,
        ),
        &self_owned,
    )
    .unwrap_or_else(|refusal| panic!("judge refused the in-memory capture: {refusal}"))
    .problems;
    assert!(
        problems.len() == 1
            && problems[0].starts_with(&format!("{self_cell} "))
            && problems[0].contains("names its own bead dc-fwpz"),
        "a row owned by dc-fwpz at {self_cell} gives {problems:?}"
    );
    note!(
        "ACCEPT-NOTE: {CASE}: on in-memory captures of {LOGICAL_WORKBOOK_PATH} (no Excel evidence; {all} rows: {computed} computed, {refused} refused): calc_verdict::judge's report through the comparison matches every computed row of the agreeing capture and counts every refused row not modeled with its owner, none of which is dc-fwpz; a capture with the other logical at {cell} is exactly one problem, at that cell; the refused row at {self_cell} given dc-fwpz as its owner is exactly one problem, at that cell"
    );

    // Excel's capture of the workbook.
    let capture = CaptureRecord::decode(LOGICAL_CAPTURE).expect("the committed capture decodes");
    assert_eq!(capture.case, LOGICAL_CAPTURE_CASE);
    assert_eq!(capture.acquisition, Acquisition::LiveCapture);
    let session = capture
        .session
        .as_ref()
        .expect("the committed capture is a version 2 record");
    let workbook = format!("sha256:{}", sha256_hex(LOGICAL_WORKBOOK));
    if capture.source != workbook {
        note!(
            "ACCEPT-NOT-RUN: {CASE}: awaiting capture: the committed capture is of {}, and the oracle workbook as committed is {workbook} (cargo xtask capture --pending takes it)",
            capture.source
        );
        return;
    }
    let isolation: Vec<_> = session.failures_at(STAGE_ISOLATION).collect();
    assert!(
        isolation.is_empty(),
        "the committed capture records isolation failures: {isolation:?}"
    );
    let environment = capture
        .environment
        .as_ref()
        .expect("the capture records Excel's environment");
    let comparison = comparison_of(&capture, &rows)
        .unwrap_or_else(|refusal| panic!("calc_verdict::judge refuses the capture: {refusal}"));
    assert!(
        comparison.problems.is_empty(),
        "{} problems on the verdict on Excel {}'s capture ({}):\n{}",
        comparison.problems.len(),
        environment.target.excel_build(),
        comparison.report.summary(),
        comparison.problems.join("\n")
    );
    let observed = comparison
        .report
        .cells
        .iter()
        .filter(|cell| {
            cell.formula.cell.sheet() == rows.sheet
                && cell.formula.cell.column() == FORMULA_COLUMN
                && matches!(cell.observation, Some(Observation::ExcelRaw(_)))
        })
        .count();
    assert_eq!(observed, all, "a raw value of Excel's for every row");
    note!(
        "ACCEPT-NOTE: {CASE}: Excel comparison through calc_verdict::judge against Excel {}'s capture of {LOGICAL_WORKBOOK_PATH}: {} (every computed row matched, every refused row not modeled with its owner); the whole workbook: {}",
        environment.target.excel_build(),
        sheet_verdicts(&comparison.report, &rows),
        comparison.report.summary()
    );
}

// ---------------------------------------------------------------------------------------------
// cdoc-jpha: the comparison operators order and equate text with Excel's collation and compare
// nearly equal numbers by Excel's rule, through calc_func::invoke on their catalogue targets; the
// lookups that ask for an exact match equate text the same way, MATCH, VLOOKUP and HLOOKUP (and
// XMATCH and XLOOKUP in wildcard mode) asking for the same length too.
//
// `tests/fixtures/cdoc_jpha/rows.txt` lists the rows, one line per row or per group of rows, with
// the value Excel gives for each: its raw value for the row's cell in the committed capture
// `tests/captures/cdoc_jpha/oracle.capture.json` ([`COLLATION_CAPTURED`]). The committed generator
// `tests/fixtures/make-fixtures.ps1` writes `tests/fixtures/cdoc_jpha/oracle.xlsx` from the same
// file: each row's formula in column D of the sheet text_and_number_comparison, in order, beside
// the cells the rows read. `cargo xtask capture --fixture
// crates/calc-func/tests/fixtures/cdoc_jpha/oracle.xlsx --case cdoc_jpha__excel_comparison --out
// crates/calc-func/tests/captures/cdoc_jpha/oracle.capture.json` wrote Excel's capture.
//
// cdoc_jpha__text_and_number_comparison runs every row through calc_func::invoke under Fast and
// asserts the value the file states, and the same call under Exact; it checks that the sheet
// holds exactly the rows' formulas and cells and, on Windows, that the generator writes the
// committed workbook. cdoc_jpha__excel_comparison publishes the rows' results and their recorded
// invoke calls to calc_verdict::judge against the capture, and asserts that every computed row
// matches, that every refused row is not modelled with its owner, and that the value each row
// states is Excel's raw value on the report.
//
// A text no formula literal holds (a control character, an invisible or a combining character, a
// space beyond ASCII) is written through CHAR or UNICHAR, joined with & to the rest of the text.
// calc_func's catalogue has neither function, and the case does not compute the & either: it
// supplies the operand's text (the character of each code), as cfunc-m1.2's computed operands are
// supplied, so for such a row only the comparison or lookup is computed through calc_func::invoke.
// The operands 0.1+0.2 and 1+1E-15 are computed through calc_func::invoke of +. Each pair of
// numbers is held in columns A and B, and the rows =+A and =+B show the numbers Excel loaded.

/// The oracle workbook of cdoc-jpha, as committed.
const COLLATION_WORKBOOK: &[u8] = include_bytes!("fixtures/cdoc_jpha/oracle.xlsx");
const COLLATION_WORKBOOK_PATH: &str = "crates/calc-func/tests/fixtures/cdoc_jpha/oracle.xlsx";
/// The rows, as the generator reads them.
const COLLATION_ROWS: &str = include_str!("fixtures/cdoc_jpha/rows.txt");
/// Excel's capture of the workbook.
const COLLATION_CAPTURE: &[u8] = include_bytes!("captures/cdoc_jpha/oracle.capture.json");
/// The case the capture is recorded under.
const COLLATION_CAPTURE_CASE: &str = "cdoc_jpha__excel_comparison";
/// How the value each row states was obtained. The comparison case asserts the capture's locale
/// and separators as [`CAPTURED_SETTINGS`] states them, and reports the build the capture records.
const COLLATION_CAPTURED: &str = "Excel's raw value for this cell in the committed capture crates/calc-func/tests/captures/cdoc_jpha/oracle.capture.json, which cdoc_jpha__excel_comparison asserts through calc_verdict::judge's report (Excel on the owner's Windows machine, whose build the capture records, under Windows locale en-ZA, user culture en-ZA; no other locale was measured)";
/// The one formula Excel reads back other than as written, beside the XLOOKUP calls it reads
/// back with @: it writes the literal 1E-15 out.
const REWRITTEN_FORMULA: (&str, &str) = ("=1+1E-15=1", "=1+0.000000000000001=1");

/// Text beyond the collation's repertoire, which the comparison refuses.
const BEYOND_REPERTOIRE: Owner = Owner {
    bead: "dc-uar",
    behaviour: "Excel's collation of characters beyond calc_func's collation repertoire (locale services)",
};

/// Nearly equal numbers in the bands where Excel's rounding departs from the exact one, which the
/// comparison refuses.
const NUMBER_BANDS: Owner = Owner {
    bead: "dc-j9k",
    behaviour: "Excel's near-equality of numbers in the bands cdoc-jpha refuses",
};

/// What a cdoc-jpha row states Excel gives, or that calc_func refuses it.
#[derive(Clone, Debug)]
enum Captured {
    Logical(bool),
    Number(f64),
    Error(WorksheetErrorCode),
    /// A text, as its UTF-16 code units.
    Text(Vec<u16>),
    /// Refused as not modelled (text beyond the repertoire), naming this argument; Excel's value
    /// is on the report.
    Refused(Option<usize>),
}

impl Captured {
    fn read(field: &str) -> Captured {
        match field {
            "T" => Captured::Logical(true),
            "F" => Captured::Logical(false),
            "#N/A" => Captured::Error(WorksheetErrorCode::NA),
            "R" => Captured::Refused(None),
            "R0" => Captured::Refused(Some(0)),
            "R1" => Captured::Refused(Some(1)),
            _ => match field.strip_prefix("s:") {
                Some(spec) => Captured::Text(collation_text(spec)),
                None => Captured::Number(
                    field
                        .parse()
                        .expect("a number, T, F, #N/A, s:text, R, R0 or R1"),
                ),
            },
        }
    }

    fn expect(&self) -> Expect {
        match self {
            Captured::Logical(b) => L(*b),
            Captured::Number(n) => N(*n),
            Captured::Error(code) => E(*code),
            Captured::Text(units) => Expect::Units(units.clone()),
            Captured::Refused(argument) => Refused(*argument),
        }
    }

    /// The value Excel gives, for a row calc_func computes.
    fn value(&self) -> Option<CalcValue> {
        match self {
            Captured::Logical(b) => Some(CalcValue::logical(*b)),
            Captured::Number(n) => Some(CalcValue::number(*n)),
            Captured::Error(code) => Some(CalcValue::error(*code)),
            Captured::Text(units) => Some(CalcValue::text(ExcelText::from_utf16_code_units(
                units.clone(),
            ))),
            Captured::Refused(_) => None,
        }
    }
}

/// A text of rows.txt: `\XXXX` is the UTF-16 code unit XXXX, `\{N}` repeats the code unit after it
/// N times, and any other character is itself.
fn collation_text(spec: &str) -> Vec<u16> {
    let bytes = spec.as_bytes();
    let mut units = Vec::new();
    let mut at = 0;
    while at < bytes.len() {
        let mut count = 1;
        if bytes[at] == b'\\' && bytes.get(at + 1) == Some(&b'{') {
            let close = at + spec[at..].find('}').expect("a closed repeat");
            count = spec[at + 2..close].parse().expect("a repeat count");
            at = close + 1;
        }
        let unit = if bytes[at] == b'\\' {
            at += 5;
            u16::from_str_radix(&spec[at - 4..at], 16).expect("four hex digits")
        } else {
            at += 1;
            u16::from(bytes[at - 1])
        };
        units.extend(std::iter::repeat_n(unit, count));
    }
    units
}

/// Whether a formula writes this code unit inside a text literal (the generator's rule): printable
/// ASCII and every code unit from U+00A0 on, except the no-break, soft hyphen and other spaces and
/// invisible characters and the combining diacritical marks.
fn collation_literal(unit: u16) -> bool {
    (0x20..=0x7E).contains(&unit)
        || (unit >= 0xA0
            && ![
                0x00A0, 0x00AD, 0x2002, 0x2003, 0x2007, 0x2009, 0x202F, 0x2060, 0xFEFF,
            ]
            .contains(&unit)
            && !(0x200B..=0x200F).contains(&unit)
            && !(0x0300..=0x036F).contains(&unit))
}

/// A text as an operand: as the formula writes it (text literals, and CHAR or UNICHAR for the
/// other code units, joined by & in parentheses when there is more than one part) and as calc_func
/// is given it.
fn collation_text_arg(units: &[u16]) -> Arg {
    let mut parts = Vec::new();
    let mut run: Vec<u16> = Vec::new();
    let literal = |run: &[u16]| {
        format!(
            "\"{}\"",
            String::from_utf16(run)
                .expect("UTF-16")
                .replace('"', "\"\"")
        )
    };
    for &unit in units {
        if collation_literal(unit) {
            run.push(unit);
        } else {
            if !run.is_empty() {
                parts.push(literal(&run));
                run.clear();
            }
            parts.push(if unit < 0x80 {
                format!("CHAR({unit})")
            } else {
                format!("UNICHAR({unit})")
            });
        }
    }
    if !run.is_empty() || parts.is_empty() {
        parts.push(literal(&run));
    }
    let excel = match parts.as_slice() {
        [one] => one.clone(),
        _ => format!("({})", parts.join("&")),
    };
    arg(
        &excel,
        CalcValue::text(ExcelText::from_utf16_code_units(units.to_vec())),
    )
}

/// A number operand written as a formula expression: a literal, or two literals joined by +, which
/// calc_func::invoke of + computes under Fast.
fn collation_number_arg(excel: &str) -> Arg {
    match excel.split_once('+') {
        None => num(excel),
        Some((a, b)) => {
            let mut given = Recorded::new(vec![
                CalcValue::number(a.parse().expect("a number literal")),
                CalcValue::number(b.parse().expect("a number literal")),
            ]);
            let value = run(surface(PLUS.0, PLUS.1), &mut given, &NumericMode::Fast)
                .unwrap_or_else(|fault| panic!("{excel} through calc_func::invoke: {fault:?}"));
            arg(excel, value)
        }
    }
}

/// The lines of rows.txt that are not comments, each split at `|`, with the number of the section
/// (the comment `# <n>. ...`) it falls in.
fn collation_lines() -> Vec<(u32, Vec<&'static str>)> {
    let mut section = 0;
    let mut lines = Vec::new();
    for line in COLLATION_ROWS.lines() {
        if let Some(comment) = line.strip_prefix("# ") {
            if let Some((number, _)) = comment.split_once(". ")
                && let Ok(number) = number.parse()
            {
                section = number;
            }
            continue;
        }
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        lines.push((section, line.split('|').collect()));
    }
    lines
}

/// The test sheet of the cdoc-jpha rows: the two numbers of each pair of section 9 in columns A
/// and B of the first of its four rows, and the constants of column C.
fn collation_sheet() -> TestSheet {
    let mut cells: Vec<(String, String, CalcValue)> = Vec::new();
    let mut row = 0u32;
    for (_, fields) in collation_lines() {
        match fields[0] {
            "t" | "f" | "c" | "COUNTIF" | "L" => row += 1,
            "N" => {
                for (column, written) in [("A", fields[1]), ("B", fields[2])] {
                    let value: f64 = written.parse().expect("a decimal");
                    cells.push((
                        format!("{column}{}", row + 1),
                        written.to_owned(),
                        CalcValue::number(value),
                    ));
                }
                row += 4;
            }
            "C" => {
                let (entry, value) = match fields[2].strip_prefix("n:") {
                    Some(written) => (
                        written.to_owned(),
                        CalcValue::number(written.parse().expect("a decimal")),
                    ),
                    None => {
                        let units = collation_text(fields[2]);
                        let entry = if units.len() > 24 {
                            format!("text of {} UTF-16 code units", units.len())
                        } else {
                            format!("text {:?}", String::from_utf16(&units).expect("UTF-16"))
                        };
                        (
                            entry,
                            CalcValue::text(ExcelText::from_utf16_code_units(units)),
                        )
                    }
                };
                cells.push((fields[1].to_owned(), entry, value));
            }
            other => panic!("rows.txt: a line of no known kind {other:?}"),
        }
    }
    let held: Vec<(&str, &str, CalcValue)> = cells
        .iter()
        .map(|(address, entry, value)| (address.as_str(), entry.as_str(), value.clone()))
        .collect();
    TestSheet::new(&held)
}

/// The cdoc-jpha rows, with the value each states Excel gives and the section each falls in, in
/// row order.
struct CollationRows {
    rows: Rows,
    captured: Vec<Captured>,
    sections: Vec<u32>,
    /// Rows whose call under NumericMode::Exact is refused with ExactUnavailable.
    exact_refused: usize,
}

/// The sections whose rows each print a note; the others print a count.
const LOUD_SECTIONS: [u32; 8] = [1, 4, 5, 7, 8, 9, 10, 11];

/// Runs one cdoc-jpha row: `surface` on `args` through calc_func::invoke under Fast, asserted to
/// give what `captured` states (for a refused row a refusal as not modelled, owned by
/// [`BEYOND_REPERTOIRE`]). The same call under Exact gives the same outcome, or is refused with
/// ExactUnavailable when it compares two different numbers or adds; a row comparing two texts has
/// the same outcome under both. Records the call and the row with the next cell of column D.
fn collation_check(
    run: &mut CollationRows,
    sheet: &mut TestSheet,
    section: u32,
    surface_of: (Notation, &str),
    args: &[Arg],
    captured: Captured,
) {
    let expect = captured.expect();
    let (formula, _, result, tick) =
        call_and_assert(sheet, &NumericMode::Fast, surface_of, args, &expect);
    let target = surface(surface_of.0, surface_of.1);
    let exact = exact_mode();
    let mut given = Recorded::new(args.iter().map(|a| a.value.clone()).collect());
    let under_exact = invoke(
        target,
        &mut given,
        &mut EvalServices::with_cells(&exact, sheet),
        &mut FunctionCallScratch::new(),
    );
    let same = match (&under_exact, &result) {
        (Ok(e), Ok(f)) => identical(e, f),
        (Err(e), Err(f)) => e == f,
        _ => false,
    };
    let texts_only = args.iter().all(|a| a.value.as_text().is_some());
    if !same {
        assert!(
            !texts_only && under_exact == Err(FunctionFault::ExactUnavailable { id: target.id() }),
            "{formula} under Exact gives {under_exact:?}, under Fast {result:?}"
        );
        run.exact_refused += 1;
    }
    let owner = match captured {
        Captured::Refused(_) => Some(BEYOND_REPERTOIRE),
        _ => None,
    };
    let rows = &mut run.rows;
    let row = u32::try_from(rows.oracle.len() + 1).expect("a row number");
    if !rows.quiet && LOUD_SECTIONS.contains(&section) {
        let reads: Vec<&str> = args
            .iter()
            .flat_map(|a| a.reads.iter().map(String::as_str))
            .collect();
        let cells = if reads.is_empty() {
            String::new()
        } else {
            format!(" with {}", reads.join(", "))
        };
        let shown = match captured {
            Captured::Refused(_) => format!(
                "refused as not modelled ({}), the same under Exact; Excel's value is on the comparison's report",
                BEYOND_REPERTOIRE.feature()
            ),
            _ if same => format!("{} as captured, the same under Exact", expect.shown()),
            _ => format!(
                "{} as captured; under Exact refused with ExactUnavailable",
                expect.shown()
            ),
        };
        note!(
            "ACCEPT-NOTE: {}: oracle {}!{} {formula}{cells} -> {shown}",
            rows.case,
            rows.sheet,
            encode_a1(row, FORMULA_COLUMN)
        );
    }
    rows.calls.push(Call { row, tick });
    rows.oracle.push(OracleRow {
        row,
        formula,
        outcome: result,
        choice: None,
        owner,
        tick,
    });
    run.captured.push(captured);
    run.sections.push(section);
}

/// The rows of cdoc_jpha__text_and_number_comparison, run through calc_func::invoke on `s`, in the
/// order the oracle workbook's sheet holds their formulas.
fn collation_rows(s: &mut TestSheet, quiet: bool) -> CollationRows {
    const CASE: &str = "cdoc_jpha__text_and_number_comparison";
    let mut run = CollationRows {
        rows: Rows::new(CASE, "text_and_number_comparison", quiet),
        captured: Vec::new(),
        sections: Vec::new(),
        exact_refused: 0,
    };
    let infix = |op: &'static str| (Notation::Infix, op);
    for (section, fields) in collation_lines() {
        let op = || -> &'static str {
            match fields[1] {
                "=" => "=",
                "<>" => "<>",
                "<" => "<",
                "<=" => "<=",
                ">" => ">",
                ">=" => ">=",
                other => panic!("rows.txt: no comparison operator {other:?}"),
            }
        };
        match fields[0] {
            "t" => {
                let args = [
                    collation_text_arg(&collation_text(fields[2])),
                    collation_text_arg(&collation_text(fields[3])),
                ];
                let captured = Captured::read(fields[4]);
                collation_check(&mut run, s, section, infix(op()), &args, captured);
            }
            "f" => {
                let args = [
                    collation_number_arg(fields[2]),
                    collation_number_arg(fields[3]),
                ];
                let captured = Captured::read(fields[4]);
                collation_check(&mut run, s, section, infix(op()), &args, captured);
            }
            "c" => {
                let args = [cell(s, fields[2]), cell(s, fields[3])];
                let captured = Captured::read(fields[4]);
                collation_check(&mut run, s, section, infix(op()), &args, captured);
            }
            "N" => {
                let first = u32::try_from(run.rows.oracle.len() + 1).expect("a row number");
                let (a, b) = (format!("A{first}"), format!("B{first}"));
                let pair = [cell(s, &a), cell(s, &b)];
                for (target, captured) in [(EQ, fields[3]), (LT, fields[4])] {
                    collation_check(
                        &mut run,
                        s,
                        section,
                        target,
                        &pair,
                        Captured::read(captured),
                    );
                }
                for (operand, written) in [(&a, fields[1]), (&b, fields[2])] {
                    let args = [cell(s, operand)];
                    let captured = Captured::read(written);
                    collation_check(&mut run, s, section, UNARY_PLUS, &args, captured);
                }
            }
            "C" => {}
            "COUNTIF" => {
                let criterion = match fields[2].split_once(':') {
                    Some(("t", spec)) => collation_text_arg(&collation_text(spec)),
                    Some(("n", literal)) => num(literal),
                    _ => panic!("rows.txt: a criterion of no known kind {:?}", fields[2]),
                };
                let ranges: Vec<&str> = vec![fields[1]];
                let args = [reference(s, &ranges), criterion];
                let captured = Captured::read(fields[3]);
                collation_check(&mut run, s, section, COUNTIF, &args, captured);
            }
            "L" => {
                let key = match fields[2].split_once(':') {
                    Some(("t", spec)) => collation_text_arg(&collation_text(spec)),
                    Some(("n", written)) => collation_number_arg(written),
                    _ => panic!("rows.txt: a key of no known kind {:?}", fields[2]),
                };
                let array = match fields[3]
                    .strip_prefix('{')
                    .and_then(|inner| inner.strip_suffix('}'))
                {
                    Some(spec) => {
                        let element = collation_text_arg(&collation_text(spec));
                        assert!(
                            element.excel.starts_with('"'),
                            "rows.txt: an array constant of literal text"
                        );
                        array_arg(&format!("{{{}}}", element.excel), vec![vec![element.value]])
                    }
                    None => reference(s, &[fields[3]]),
                };
                let mut args = vec![key, array];
                if !fields[4].is_empty() {
                    for more in fields[4].split(',') {
                        args.push(match more {
                            "" => omitted(),
                            "TRUE" => logical(true),
                            "FALSE" => logical(false),
                            _ => match more.strip_prefix('{').and_then(|n| n.strip_suffix('}')) {
                                Some(number) => array_arg(
                                    more,
                                    vec![vec![CalcValue::number(
                                        number.parse().expect("a number"),
                                    )]],
                                ),
                                None => num(more),
                            },
                        });
                    }
                }
                let captured = Captured::read(fields[5]);
                collation_check(
                    &mut run,
                    s,
                    section,
                    (Notation::Call, fields[1]),
                    &args,
                    captured,
                );
            }
            other => panic!("rows.txt: a line of no known kind {other:?}"),
        }
    }
    run
}

/// Asserts that comparing `a` and `b` by = and by < is refused as not modelled, naming no
/// argument (owned by [`NUMBER_BANDS`]), and returns how many calls that was.
fn collation_refuses(a: f64, b: f64) -> usize {
    for target in [EQ, LT] {
        let target = surface(target.0, target.1);
        let mut given = Recorded::new(vec![CalcValue::number(a), CalcValue::number(b)]);
        assert_eq!(
            run(target, &mut given, &NumericMode::Fast),
            Err(FunctionFault::NotImplemented {
                id: target.id(),
                argument: None
            }),
            "{a:?} and {b:?}"
        );
    }
    2
}

#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test)]
fn cdoc_jpha__text_and_number_comparison() {
    const CASE: &str = "cdoc_jpha__text_and_number_comparison";
    let mut sheet = collation_sheet();
    let collated = collation_rows(&mut sheet, false);
    let rows = &collated.rows;
    assert!(
        rows.oracle
            .iter()
            .all(|row| row.owner.is_some() == row.outcome.is_err()),
        "a refused row names its owner, a computed row none"
    );

    // Each section's rows, counted, with how many state TRUE.
    let mut sections: BTreeMap<u32, (usize, usize)> = BTreeMap::new();
    for (section, captured) in collated.sections.iter().zip(&collated.captured) {
        let entry = sections.entry(*section).or_default();
        entry.0 += 1;
        if matches!(captured, Captured::Logical(true)) {
            entry.1 += 1;
        }
    }
    let counted: Vec<String> = sections
        .iter()
        .map(|(section, (all, true_rows))| {
            format!("section {section}: {all} rows ({true_rows} stating TRUE)")
        })
        .collect();
    note!(
        "ACCEPT-NOTE: {CASE}: rows by section of tests/fixtures/cdoc_jpha/rows.txt (1 cfunc-m1.2's 14 comparison kinds, 2 every character of the repertoire in Excel's order, 3 the level probes of each step of that order, 4 ligatures, 5 the first pass, special characters, spaces, punctuation and labels, 6 random pairs, 7 long texts, 8 text beyond the repertoire, 9 numbers, 10 COUNTIF criteria, 11 lookups' equality; the rows of sections 2, 3 and 6 are counted here, the others noted one by one above): {}",
        counted.join(", ")
    );

    // The operands the case supplies rather than computes: text written through CHAR or UNICHAR.
    let computed: Vec<&OracleRow> = rows
        .oracle
        .iter()
        .filter(|row| row.outcome.is_ok())
        .collect();
    let through_char: Vec<&&OracleRow> = computed
        .iter()
        .filter(|row| row.formula.contains("CHAR("))
        .collect();
    let joined = through_char
        .iter()
        .filter(|row| row.formula.contains('&'))
        .count();
    note!(
        "ACCEPT-NOTE: {CASE}: {} of the {} computed rows write an operand through CHAR or UNICHAR ({joined} of them joining it to other text with &), which calc_func's catalogue does not have: for these the case supplies the operand's text (the character of each code) and only the comparison or lookup is computed through calc_func::invoke, as cfunc-m1.2's computed operands are supplied",
        through_char.len(),
        computed.len()
    );

    // The oracle workbook: its sheet holds exactly the rows' formulas (XMATCH and XLOOKUP with
    // the file's _xlfn. prefix) and cells, and the committed workbook is its generator's output.
    assert_workbook_sheet(
        COLLATION_WORKBOOK,
        COLLATION_WORKBOOK_PATH,
        &stored_formulas(rows),
        &sheet,
    );
    #[cfg(all(windows, not(target_arch = "wasm32")))]
    {
        let generated = generated_fixtures();
        let names: Vec<&str> = generated.keys().map(String::as_str).collect();
        assert_eq!(
            names, GENERATED_WORKBOOKS,
            "the generator writes the oracle workbooks and nothing else"
        );
        let regenerated = &generated["cdoc_jpha/oracle.xlsx"];
        assert!(
            regenerated.as_slice() == COLLATION_WORKBOOK,
            "the committed cdoc-jpha oracle workbook is not the generator's output (sha256 {} committed, {} generated)",
            sha256_hex(COLLATION_WORKBOOK),
            sha256_hex(regenerated)
        );
        note!(
            "ACCEPT-NOTE: {CASE}: crates/calc-func/tests/fixtures/make-fixtures.ps1, run now under Windows PowerShell, wrote the committed cdoc-jpha oracle workbook byte for byte ({} bytes) from tests/fixtures/cdoc_jpha/rows.txt",
            regenerated.len()
        );
    }
    #[cfg(not(all(windows, not(target_arch = "wasm32"))))]
    {
        note!(
            "ACCEPT-NOTE: {CASE}: the generator runs under Windows PowerShell, so the check that the committed oracle workbook is its output runs on the windows route; here the workbook is checked against the rows only"
        );
    }

    // Through the call interface alone, numbers where Excel's own rounding departs from the exact
    // one: these comparisons are refused, and numbers far apart there still order by value.
    let top = f64::from_bits(0x7FE0_0000_0000_0000);
    let after = |x: f64, ulps: u64| f64::from_bits(x.to_bits() + ulps);
    let mut refused = 0;
    refused += collation_refuses(top, after(top, 3));
    refused += collation_refuses(f64::MAX, f64::MAX.next_down());
    refused += collation_refuses(1e-146, after(1e-146, 10));
    refused += collation_refuses(1.001e-146, after(1.001e-146, 1));
    refused += collation_refuses(1e-292, after(1e-292, 10));
    refused += collation_refuses(9.531725029864735e200, 9.53172502986473e200);
    refused += collation_refuses(9.531725029864735e200, 9.53172502986474e200);
    for (a, b, less) in [
        (f64::MAX, 1.0, false),
        (1e-146, 1e-145, true),
        (-f64::MAX, top, true),
    ] {
        let mut given = Recorded::new(vec![CalcValue::number(a), CalcValue::number(b)]);
        assert_eq!(
            run(surface(LT.0, LT.1), &mut given, &NumericMode::Fast),
            Ok(CalcValue::logical(less)),
            "{a:?} < {b:?}"
        );
    }
    note!(
        "ACCEPT-NOTE: {CASE}: through the call interface alone (no oracle row, not compared with Excel here): {refused} calls comparing nearly equal numbers where Excel's rounding departs from the exact one are refused as not modelled, naming no argument, owned by {}: in binary64's top binade (2^1023 and 3 ulps above it; the largest number and the one below it), from 1E-146 (and 10 ulps above it; 1.001E-146) and from 1E-292, and 9.531725029864735E+200 against either 15-digit neighbour, which lies a relative 2^-64.5 below a 16th-digit tie (the band within a relative 2^-59 of a tie); numbers far apart there order by value. The departures were read in Excel COM sessions while this bead was built, not in the committed capture",
        NUMBER_BANDS.feature()
    );

    // Text holding U+0000 (which CHAR(0) and UNICHAR(0) do not give: both are #VALUE! in Excel) is
    // not decided (Windows' sort key ignores it; Excel's comparison of it is not measured): refused,
    // naming the operand that holds it. calc_func's comparison before this bead equated ASCII
    // texts, U+0000 included, ignoring ASCII case.
    let nul = |text: &str| CalcValue::text(ExcelText::from_interop_assignment(text));
    let mut narrowed = 0;
    for (target, a, b, argument) in [
        (EQ, "a\u{0}", "a", 0),
        (EQ, "A", "a\u{0}", 1),
        (LT, "a\u{0}b", "A\u{0}B", 0),
    ] {
        let target = surface(target.0, target.1);
        let mut given = Recorded::new(vec![nul(a), nul(b)]);
        assert_eq!(
            run(target, &mut given, &NumericMode::Fast),
            Err(FunctionFault::NotImplemented {
                id: target.id(),
                argument: Some(argument)
            }),
            "{a:?} and {b:?}"
        );
        narrowed += 1;
    }
    note!(
        "ACCEPT-NOTE: {CASE}: through the call interface alone (not compared with Excel): {narrowed} comparisons of text holding U+0000 (which neither CHAR(0) nor UNICHAR(0) gives) are refused as not modelled, naming the operand that holds it, owned by {}; before this bead calc_func's comparison computed such ASCII pairs, so this is a narrowing, counted here",
        BEYOND_REPERTOIRE.feature()
    );
    note!(
        "ACCEPT-NOTE: {CASE}: {} rows through calc_func::invoke: {} computed, each equal to {COLLATION_CAPTURED}; {} refused as not modelled ({}); under NumericMode::Exact {} rows comparing two different numbers or adding are refused with ExactUnavailable and every other row gives its Fast outcome",
        rows.oracle.len(),
        rows.oracle.len() - rows.refused(),
        rows.refused(),
        BEYOND_REPERTOIRE.feature(),
        collated.exact_refused
    );
}

#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test)]
fn cdoc_jpha__excel_comparison() {
    const CASE: &str = "cdoc_jpha__excel_comparison";
    let CollationRows { rows, captured, .. } = collation_rows(&mut collation_sheet(), true);
    let (results, evaluations) = published_in(COLLATION_WORKBOOK, COLLATION_CAPTURE_CASE, &[&rows]);
    assert_eq!(
        (results.results.len(), evaluations.evaluations.len()),
        (rows.oracle.len(), rows.oracle.len()),
        "a result and an evaluation for each row"
    );
    let not_modelled = results
        .results
        .iter()
        .filter(|result| matches!(result.provenance, Provenance::NotModeled { .. }))
        .count();
    assert_eq!(
        not_modelled,
        rows.refused(),
        "every refused row is published as not modelled, and only those"
    );
    let capture = CaptureRecord::decode(COLLATION_CAPTURE).expect("the committed capture decodes");
    assert_eq!(capture.case, COLLATION_CAPTURE_CASE);
    assert_eq!(capture.acquisition, Acquisition::LiveCapture);
    let session = capture
        .session
        .as_ref()
        .expect("the committed capture is a version 2 record");
    let workbook = format!("sha256:{}", sha256_hex(COLLATION_WORKBOOK));
    if capture.source != workbook {
        note!(
            "ACCEPT-NOT-RUN: {CASE}: awaiting capture: the committed capture is of {}, and the oracle workbook as committed is {workbook} (cargo xtask capture --pending takes it)",
            capture.source
        );
        return;
    }
    let isolation: Vec<_> = session.failures_at(STAGE_ISOLATION).collect();
    assert!(
        isolation.is_empty(),
        "the committed capture records isolation failures: {isolation:?}"
    );
    let environment = capture
        .environment
        .as_ref()
        .expect("the capture records Excel's environment");
    let comparison = judged(&capture, &results, &evaluations, &rows)
        .unwrap_or_else(|refusal| panic!("calc_verdict::judge refuses the capture: {refusal}"));
    assert!(
        comparison.problems.is_empty(),
        "{} problems on the verdict on Excel {}'s capture ({}):\n{}",
        comparison.problems.len(),
        environment.target.excel_build(),
        comparison.report.summary(),
        comparison.problems.join("\n")
    );

    // The value each row states is the raw value the report carries for the row's cell.
    let observed: BTreeMap<u32, &Observation> = comparison
        .report
        .cells
        .iter()
        .filter(|cell| {
            cell.formula.cell.sheet() == rows.sheet && cell.formula.cell.column() == FORMULA_COLUMN
        })
        .filter_map(|cell| Some((cell.formula.cell.row(), cell.observation.as_ref()?)))
        .collect();
    assert_eq!(observed.len(), rows.oracle.len(), "an observation per row");
    let mut refused_values = Vec::new();
    for (row, stated) in rows.oracle.iter().zip(&captured) {
        let at = format!("{}!{}", rows.sheet, encode_a1(row.row, FORMULA_COLUMN));
        let Some(Observation::ExcelRaw(value)) = observed.get(&row.row) else {
            panic!("{at}: the report carries no raw value of Excel's");
        };
        match stated.value() {
            Some(expected) => assert!(
                identical(value, &expected),
                "{at} {}: the row states {}, the capture holds {}",
                row.formula,
                shown_value(&expected),
                shown_value(value)
            ),
            None => refused_values.push(format!("{at} {} {}", row.formula, shown_value(value))),
        }
    }
    note!(
        "ACCEPT-NOTE: {CASE}: every computed row's stated value is Excel's raw value for its cell on the report; the {} refused rows' values in Excel: {}",
        refused_values.len(),
        refused_values.join("; ")
    );

    // Excel read every formula back as written, but for the one whose literal it writes out and
    // the XLOOKUP calls, which it reads back with the implicit-intersection operator @: the file
    // stores every formula without dynamic-array metadata.
    let mut rewritten = Vec::new();
    let mut intersected = 0;
    for row in &rows.oracle {
        let read_back = capture
            .formulas
            .iter()
            .find(|formula| {
                formula.cell.sheet() == rows.sheet
                    && (formula.cell.row(), formula.cell.column()) == (row.row, FORMULA_COLUMN)
            })
            .and_then(|formula| formula.formula.as_deref());
        if read_back == Some(row.formula.as_str()) {
            continue;
        }
        if let Some(call) = row.formula.strip_prefix("=XLOOKUP(") {
            assert_eq!(
                read_back,
                Some(format!("=@XLOOKUP({call}").as_str()),
                "Excel's formula of row {}",
                row.row
            );
            intersected += 1;
            continue;
        }
        assert_eq!(
            (row.formula.as_str(), read_back),
            (REWRITTEN_FORMULA.0, Some(REWRITTEN_FORMULA.1)),
            "Excel's formula of row {}",
            row.row
        );
        rewritten.push(row.row);
    }
    assert_eq!(rewritten.len(), 1, "the one rewritten formula");

    // What the label of the rows' values states of the capture.
    let setting = |name: &str| {
        environment
            .settings
            .iter()
            .find(|(setting, _)| setting == name)
            .map(|(_, value)| value.as_str())
    };
    for (name, value) in CAPTURED_SETTINGS {
        assert_eq!(setting(name), Some(value), "the capture's setting {name}");
    }
    note!(
        "ACCEPT-NOTE: {CASE}: the capture was taken by Excel {} under {}; Excel read every formula back as written but row {} ({} reads back as {}) and the {intersected} XLOOKUP formulas, each read back with the implicit-intersection operator @ (=@XLOOKUP(...))",
        environment.target.excel_build(),
        CAPTURED_SETTINGS
            .iter()
            .map(|(name, value)| format!("{name} {value:?}"))
            .collect::<Vec<_>>()
            .join(", "),
        encode_a1(rewritten[0], FORMULA_COLUMN),
        REWRITTEN_FORMULA.0,
        REWRITTEN_FORMULA.1
    );
    note!(
        "ACCEPT-NOTE: {CASE}: Excel comparison through calc_verdict::judge against Excel {}'s capture of the oracle workbook: {} (every computed row matched, every refused row not modeled with its owner); the whole workbook: {}",
        environment.target.excel_build(),
        sheet_verdicts(&comparison.report, &rows),
        comparison.report.summary()
    );
}

// ---------------------------------------------------------------------------------------------
// dc-nzpw: the information functions (ISBLANK, ISERR, ISERROR, ISNA, ISNUMBER, ISTEXT, ISNONTEXT,
// ISLOGICAL, ISEVEN, ISODD, ISREF, ISFORMULA, TYPE, N, NA and ERROR.TYPE) through
// calc_func::invoke on catalogue targets.
//
// Each oracle row runs one call and asserts the value it expects, and says why. A `documented`
// row applies a rule or an example that Microsoft's page for the function states, named in the
// row. A `measured` row expects what Excel gives where no page settles it (a blank cell or empty
// text where a page is silent, number text and logicals given to ISEVEN, a logical given to
// ISNUMBER, the newer errors, a reference of several cells or areas, ERROR.TYPE's numbers past
// #GETTING_DATA, the tolerance of ISEVEN and ISODD): its expectation is Excel's raw value for the
// row's cell in the committed capture, which dc_nzpw__excel_comparison asserts through
// calc_verdict::judge's report. A `refused` row ends in a fault and names the bead that owns the
// behaviour it refuses, never dc-nzpw itself; the comparison publishes it as not modelled with
// that owner. An `unrecorded` row, N(B9), computes
// #BUSY!, whose CVErr code the capture's value model does not hold: it is published not modelled
// with its owner, and the comparison checks the capture's value failure naming the code and
// Excel's saved cache of the cell instead. The comparison also checks each formula's text as
// Excel loaded it, where Excel inserts `@`, and which rows are dynamic-array formulas.
//
// A row that reads a source cell of column F (FILTER with no match, #CALC!; SEQUENCE blocked by
// F3, #SPILL!; FIELDVALUE of a number, #FIELD!) reads the error the case's sheet holds for it,
// the error it assumes Excel gives the source: calc_func has no entry for those functions, so no
// route of the product calculates the source. Such a row is published not modelled, conditional
// on Excel's value of its source, and the comparison checks both that Excel's raw value of the
// source is the assumed error and that the row's raw value is the value calc_func::invoke gave.
// Row 3, IF(A4,"x",""), gives empty text through calc_func::invoke, and the rows that read D3
// read that computed value: a formula's empty text.
//
// `tests/fixtures/dc_nzpw/oracle.xlsx`, written by the committed generator
// `tests/fixtures/make-fixtures.ps1`, holds the cells the rows read, the source formulas of column
// F (as dynamic-array formulas) and every row's formula in column D, in the order the case runs its
// rows (rows 171 to 173 as dynamic-array formulas, the others as legacy formulas);
// dc_nzpw__information_functions checks that its sheet holds exactly those. `cargo xtask
// capture` wrote Excel's capture of that workbook to `tests/captures/dc_nzpw/oracle.capture.json`.

/// The information oracle workbook, as committed.
const INFORMATION_WORKBOOK: &[u8] = include_bytes!("fixtures/dc_nzpw/oracle.xlsx");
const INFORMATION_WORKBOOK_PATH: &str = "crates/calc-func/tests/fixtures/dc_nzpw/oracle.xlsx";
/// Excel's capture of the information oracle workbook.
const INFORMATION_CAPTURE: &[u8] = include_bytes!("captures/dc_nzpw/oracle.capture.json");
/// The case the information capture is recorded under.
const INFORMATION_CAPTURE_CASE: &str = "dc_nzpw__excel_comparison";
/// The command that writes the information capture.
const INFORMATION_CAPTURE_COMMAND: &str = "cargo xtask capture --fixture crates/calc-func/tests/fixtures/dc_nzpw/oracle.xlsx --case dc_nzpw__excel_comparison --out crates/calc-func/tests/captures/dc_nzpw/oracle.capture.json";
/// The sheet of the information oracle workbook that holds the rows.
const INFORMATION_SHEET: &str = "information_functions";
/// The column (F) of the source formulas.
const SOURCE_COLUMN: u32 = 6;

/// The family's entries: each spelling and the member number of its FunctionId in family 10.
const INFORMATION: [(&str, u32); 16] = [
    ("ISBLANK", 1),
    ("ISERR", 2),
    ("ISERROR", 3),
    ("ISNA", 4),
    ("ISNUMBER", 5),
    ("ISTEXT", 6),
    ("ISNONTEXT", 7),
    ("ISLOGICAL", 8),
    ("ISEVEN", 9),
    ("ISODD", 10),
    ("ISREF", 11),
    ("ISFORMULA", 12),
    ("TYPE", 13),
    ("N", 14),
    ("NA", 15),
    ("ERROR.TYPE", 16),
];

/// Why a dc-nzpw row expects its value.
#[derive(Clone, Copy, Debug)]
enum InfoBasis {
    /// This rule, which Microsoft's page for the function states.
    Documented(&'static str),
    /// What Excel gives, which no page settles: the row's value in the committed capture.
    Measured(&'static str),
    /// Refused; the behaviour is this bead's.
    Refused(Owner),
    /// Computed by this rule, which Microsoft's page states, but Excel's raw value is an error the
    /// capture's value model holds no code for: the row is published not modelled with this
    /// owner, and the comparison checks Excel's saved cache of the cell and the capture's failure
    /// naming the error's code instead.
    Unrecorded(&'static str, Owner),
}

impl InfoBasis {
    /// The bead that owns what the row does not publish as calculated, if any.
    fn owner(self) -> Option<Owner> {
        match self {
            InfoBasis::Refused(owner) | InfoBasis::Unrecorded(_, owner) => Some(owner),
            InfoBasis::Documented(_) | InfoBasis::Measured(_) => None,
        }
    }
}

/// Excel's CVErr code for #BUSY! (the low 16 bits of the VT_ERROR value), which bead dc-nzpw's
/// capture records in its failure at the row reading B9.
const BUSY_CODE: u64 = 2051;

/// A source formula of column F: a call to a function calc_func has no entry for, which the
/// workbook holds as a dynamic-array formula over its one cell. Rows read the error the case's
/// sheet holds for it, `assumed`; the comparison checks Excel's raw value of the cell against it.
struct InfoSource {
    address: &'static str,
    row: u32,
    formula: &'static str,
    assumed: WorksheetErrorCode,
    owner: Owner,
}

const INFO_SOURCES: [InfoSource; 3] = [
    InfoSource {
        address: "F1",
        row: 1,
        formula: "=FILTER(C9:C10,C9:C10>100)",
        assumed: WorksheetErrorCode::Calc,
        owner: Owner {
            bead: "dc-j9k",
            behaviour: "FILTER (no row matches: #CALC!)",
        },
    },
    InfoSource {
        address: "F2",
        row: 2,
        formula: "=SEQUENCE(2)",
        assumed: WorksheetErrorCode::Spill,
        owner: Owner {
            bead: "dc-j9k, dc-10y",
            behaviour: "SEQUENCE and its spill (blocked by F3: #SPILL!)",
        },
    },
    InfoSource {
        address: "F4",
        row: 4,
        formula: "=FIELDVALUE(A3,\"x\")",
        assumed: WorksheetErrorCode::Field,
        owner: Owner {
            bead: "dc-uar",
            behaviour: "FIELDVALUE (A3 holds no data type: #FIELD!)",
        },
    },
];

/// An array constant written in a legacy formula's row, as Excel loads it there (`@{2,3}`, see
/// [`INFORMATION_LOADED`]): its first element. CalcFml's route gives the same value: ISEVEN, ISODD
/// and N apply to each element and the formula's root takes the first result (dc-b1l).
fn loaded_first(excel: &str, first: CalcValue) -> Arg {
    Arg {
        excel: excel.to_owned(),
        value: first,
        reads: Vec::new(),
    }
}

/// Why a legacy formula's row over an array constant gives the first element's value.
const LEGACY_FIRST: &str =
    "Excel loads the legacy formula with @ before the array, its first element (dc-b1l)";

/// The behaviours refused rows of dc-nzpw show, by the bead that owns each.
const ARRAY_ARGUMENT: Owner = Owner {
    bead: "dc-10y",
    behaviour: "an array given to a value argument, which Excel loads in a legacy formula with @ (its first element) and which array evaluation applies the function to in a dynamic-array formula",
};
const FORMULA_FACT: Owner = Owner {
    bead: "dc-uar",
    behaviour: "whether a cell holds a formula, a host fact the cell reader does not supply",
};
const RANGE_TO_VALUE: Owner = Owner {
    bead: "dc-10y",
    behaviour: "a range given to a value argument in a legacy formula, which Excel loads with @ (implicit intersection) and the caller intersects before the call",
};
const BUSY_RAW: Owner = Owner {
    bead: "dc-uar",
    behaviour: "#BUSY! as Excel's raw value: its CVErr code 2051 is none the capture's value model holds, so the comparison checks Excel's saved cache of the cell",
};

/// The rows of a dc-nzpw case, with each row's basis, the source cell a conditional row reads and
/// the rows written as dynamic-array formulas (every other row is a legacy formula).
struct InfoRows {
    rows: Rows,
    basis: Vec<InfoBasis>,
    sources_read: BTreeMap<u32, &'static str>,
    dynamic: BTreeSet<u32>,
}

impl InfoRows {
    fn count(&self, kind: fn(&InfoBasis) -> bool) -> usize {
        self.basis.iter().filter(|basis| kind(basis)).count()
    }
}

/// What a dc-nzpw row asserts of its call: a value or a refusal as [`Expect`] states it, or the
/// call interface's refusal of a reference given to the entry's value parameter, which the
/// caller dereferences (`FunctionFault::ReferenceArgument` naming argument 0).
enum InfoCheck {
    Expect(Expect),
    ReferenceArgument,
}

impl InfoCheck {
    fn shown(&self) -> String {
        match self {
            InfoCheck::Expect(expect) => information_shown(expect),
            InfoCheck::ReferenceArgument => "ReferenceArgument (argument 0)".to_owned(),
        }
    }
}

/// Runs the named call `spelling` on `args` through calc_func::invoke under Fast with the test
/// sheet as the cell reader and asserts `check`, that an eager call asked for each argument once
/// in order, and that the result fits `basis` (a value, or a refusal for a refused row). Then
/// records the row with the next cell of column D, prints it with its basis and returns its row.
fn info_row(
    r: &mut InfoRows,
    sheet: &mut TestSheet,
    spelling: &'static str,
    args: &[Arg],
    check: InfoCheck,
    basis: InfoBasis,
) -> u32 {
    let target = surface(Notation::Call, spelling);
    let (formula, asked, result, tick) = match &check {
        InfoCheck::Expect(expect) => {
            call_and_assert(sheet, &NumericMode::Fast, named(spelling), args, expect)
        }
        InfoCheck::ReferenceArgument => {
            let texts: Vec<&str> = args.iter().map(|a| a.excel.as_str()).collect();
            let formula = target
                .oracle_formula(&texts)
                .unwrap_or_else(|| panic!("{spelling} does not admit {texts:?}"));
            let mut recorded = Recorded::new(args.iter().map(|a| a.value.clone()).collect());
            let tick = next_tick();
            let result = invoke(
                target,
                &mut recorded,
                &mut EvalServices::with_cells(&NumericMode::Fast, sheet),
                &mut FunctionCallScratch::new(),
            );
            assert_eq!(
                result,
                Err(FunctionFault::ReferenceArgument {
                    id: target.id(),
                    index: 0
                }),
                "{formula}"
            );
            (formula, recorded.asked, result, tick)
        }
    };
    if !target.argument_demand().is_lazy() {
        let in_order: Vec<usize> = (0..args.len()).collect();
        assert_eq!(asked, in_order, "{formula}: the arguments asked for");
    }
    match basis {
        InfoBasis::Refused(owner) => {
            assert!(
                result.is_err(),
                "{formula}: a refused row computed {result:?}"
            );
            assert!(
                !owner.bead.split(", ").any(|bead| bead == "dc-nzpw"),
                "{formula}: a refused row names dc-nzpw as its owner"
            );
        }
        InfoBasis::Unrecorded(_, owner) => {
            assert!(
                result.is_ok(),
                "{formula}: a row of dc-nzpw computes a value"
            );
            assert!(
                !owner.bead.split(", ").any(|bead| bead == "dc-nzpw"),
                "{formula}: an unrecorded row names dc-nzpw as its owner"
            );
        }
        InfoBasis::Documented(_) | InfoBasis::Measured(_) => {
            assert!(
                result.is_ok(),
                "{formula}: a row of dc-nzpw computes a value"
            )
        }
    }
    let source = args.iter().find_map(|a| {
        INFO_SOURCES
            .iter()
            .find(|source| source.address == a.excel)
            .map(|source| source.address)
    });
    let row = u32::try_from(r.rows.oracle.len() + 1).expect("a row number");
    if !r.rows.quiet {
        let reads: Vec<&str> = args
            .iter()
            .flat_map(|a| a.reads.iter().map(String::as_str))
            .collect();
        let cells = if reads.is_empty() {
            String::new()
        } else {
            format!(" with {}", reads.join(", "))
        };
        let at = format!("{}!{}", r.rows.sheet, encode_a1(row, FORMULA_COLUMN));
        let label = match basis {
            InfoBasis::Documented(rule) => format!("documented: {rule}"),
            InfoBasis::Measured(note) => format!(
                "measured: {note}; the expectation is Excel's raw value of the cell, which dc_nzpw__excel_comparison asserts against the committed capture"
            ),
            InfoBasis::Refused(owner) => format!(
                "refused with {:?}, owned by {}: {}",
                result.as_ref().err(),
                owner.bead,
                owner.behaviour
            ),
            InfoBasis::Unrecorded(rule, owner) => format!(
                "documented: {rule}; published not modelled, owned by {}: {}",
                owner.bead, owner.behaviour
            ),
        };
        let conditional = match source {
            Some(read) => format!(
                "; conditional on Excel's value of {read}, the assumed error the sheet holds for it"
            ),
            None => String::new(),
        };
        note!(
            "ACCEPT-NOTE: {}: oracle {at} {formula}{cells} -> {} ({label}{conditional})",
            r.rows.case,
            check.shown()
        );
    }
    if let Some(read) = source {
        r.sources_read.insert(row, read);
    }
    r.rows.calls.push(Call { row, tick });
    r.rows.oracle.push(OracleRow {
        row,
        formula,
        outcome: result,
        choice: None,
        owner: basis.owner(),
        tick,
    });
    r.basis.push(basis);
    row
}

/// Every worksheet error's literal, as Excel shows it.
const ALL_ERROR_LITERALS: [(WorksheetErrorCode, &str); 14] = [
    (WorksheetErrorCode::Null, "#NULL!"),
    (WorksheetErrorCode::Div0, "#DIV/0!"),
    (WorksheetErrorCode::Value, "#VALUE!"),
    (WorksheetErrorCode::Ref, "#REF!"),
    (WorksheetErrorCode::Name, "#NAME?"),
    (WorksheetErrorCode::Num, "#NUM!"),
    (WorksheetErrorCode::NA, "#N/A"),
    (WorksheetErrorCode::Busy, "#BUSY!"),
    (WorksheetErrorCode::GettingData, "#GETTING_DATA"),
    (WorksheetErrorCode::Spill, "#SPILL!"),
    (WorksheetErrorCode::Calc, "#CALC!"),
    (WorksheetErrorCode::Field, "#FIELD!"),
    (WorksheetErrorCode::Blocked, "#BLOCKED!"),
    (WorksheetErrorCode::Connect, "#CONNECT!"),
];

/// An expectation as a dc-nzpw note shows it, an error of any kind by its literal.
fn information_shown(expect: &Expect) -> String {
    match expect {
        E(code) => ALL_ERROR_LITERALS
            .iter()
            .find(|(known, _)| known == code)
            .map(|(_, literal)| (*literal).to_owned())
            .expect("every error has a literal"),
        other => other.shown(),
    }
}

/// Runs a row of dc-nzpw and gives its row: `info!(r, sheet, "SPELLING", [args], expect, basis)`,
/// or, for a reference given to a value parameter, `info!(r, sheet, "SPELLING", [args],
/// reference_argument, basis)`.
macro_rules! info {
    ($r:expr, $sheet:expr, $spelling:expr, [$($arg:expr),* $(,)?], reference_argument, $basis:expr) => {{
        let args = [$($arg),*];
        info_row($r, $sheet, $spelling, &args, InfoCheck::ReferenceArgument, $basis)
    }};
    ($r:expr, $sheet:expr, $spelling:expr, [$($arg:expr),* $(,)?], $expect:expr, $basis:expr) => {{
        let args = [$($arg),*];
        info_row($r, $sheet, $spelling, &args, InfoCheck::Expect($expect), $basis)
    }};
}

/// The cells the information rows read: A1 is blank, A2 holds empty text, A3:A12 one value of each
/// kind and number text (A10 is blank), B1:B9 one error of each kind a cell holds as a constant,
/// C1:C8 integers at and past 2^52, the largest binary64, -2.5 and number text, C9:C10 FILTER's
/// source, C11 and C12 the binary64 two and three steps below 300001, C13 the binary64 one step
/// below 435, E167:E174 odd and even integers for the ranges of rows 168 to 173; D3 holds row 3's
/// result, F1, F2 and F4 the errors assumed for the source formulas, and F3 the text that blocks
/// F2's spill.
fn information_sheet() -> TestSheet {
    use WorksheetErrorCode as W;
    let e = CalcValue::error;
    let n = CalcValue::number;
    TestSheet::new(&[
        ("A2", "empty text", text_value("")),
        ("A3", "0", n(0.0)),
        ("A4", "FALSE", CalcValue::logical(false)),
        ("A5", "TRUE", CalcValue::logical(true)),
        ("A6", "text \"2\"", text_value("2")),
        ("A7", "text \"a\"", text_value("a")),
        ("A8", "2.5", n(2.5)),
        ("A9", "-3", n(-3.0)),
        ("A11", "text \"TRUE\"", text_value("TRUE")),
        ("A12", "text \"1e3\"", text_value("1e3")),
        ("B1", "#NULL!", e(W::Null)),
        ("B2", "#DIV/0!", e(W::Div0)),
        ("B3", "#VALUE!", e(W::Value)),
        ("B4", "#REF!", e(W::Ref)),
        ("B5", "#NAME?", e(W::Name)),
        ("B6", "#NUM!", e(W::Num)),
        ("B7", "#N/A", e(W::NA)),
        ("B8", "#GETTING_DATA", e(W::GettingData)),
        ("B9", "#BUSY!", e(W::Busy)),
        ("C1", "9007199254740991", n(9_007_199_254_740_991.0)),
        ("C2", "1.7976931348623157E+308", n(f64::MAX)),
        ("C3", "-9007199254740991", n(-9_007_199_254_740_991.0)),
        ("C4", "4503599627370497", n(4_503_599_627_370_497.0)),
        ("C5", "1E+300", n(1e300)),
        ("C6", "-2.5", n(-2.5)),
        ("C7", "text \"1e308\"", text_value("1e308")),
        ("C8", "text \"1/2\"", text_value("1/2")),
        ("C9", "0", n(0.0)),
        ("C10", "2", n(2.0)),
        (
            "C11",
            "300000.99999999988, the binary64 two steps below 300001",
            n(f64::from_bits(300_001.0_f64.to_bits() - 2)),
        ),
        (
            "C12",
            "300000.99999999983, the binary64 three steps below 300001",
            n(f64::from_bits(300_001.0_f64.to_bits() - 3)),
        ),
        (
            "C13",
            "434.99999999999994, the binary64 one step below 435",
            n(f64::from_bits(435.0_f64.to_bits() - 1)),
        ),
        ("E167", "3", n(3.0)),
        ("E168", "2", n(2.0)),
        ("E169", "5", n(5.0)),
        ("E170", "4", n(4.0)),
        ("E171", "7", n(7.0)),
        ("E172", "6", n(6.0)),
        ("E173", "9", n(9.0)),
        ("E174", "8", n(8.0)),
        (
            "D3",
            "the formula =IF(A4,\"x\",\"\") of row 3, giving empty text through calc_func::invoke",
            text_value(""),
        ),
        (
            "F1",
            "the formula =FILTER(C9:C10,C9:C10>100), assumed #CALC!",
            e(W::Calc),
        ),
        (
            "F2",
            "the formula =SEQUENCE(2), assumed #SPILL! (F3 blocks it)",
            e(W::Spill),
        ),
        ("F3", "text \"block\"", text_value("block")),
        (
            "F4",
            "the formula =FIELDVALUE(A3,\"x\"), assumed #FIELD!",
            e(W::Field),
        ),
    ])
}

// Rules the rows apply, as the functions' pages state them.
const IS_BLANK: &str = "ISBLANK is TRUE when value refers to an empty cell (the IS functions page)";
const IS_NOT_BLANK: &str = "ISBLANK is TRUE only when value refers to an empty cell; this cell holds a value (the IS functions page)";
const IS_ERR: &str = "ISERR is TRUE for any error value except #N/A (the IS functions page)";
const IS_ERROR: &str = "ISERROR is TRUE for any error value (the IS functions page)";
const IS_NA: &str = "ISNA is TRUE for the #N/A error value alone (the IS functions page)";
const NOT_CONVERTED: &str = "the IS functions' value is not converted: a number in quotation marks is text, as ISNUMBER(\"19\") is FALSE (the IS functions page)";
const IS_NUMBER: &str = "ISNUMBER is TRUE when value is a number (the IS functions page)";
const IS_TEXT: &str = "ISTEXT is TRUE when value is text (the IS functions page)";
const IS_NONTEXT: &str = "ISNONTEXT is TRUE for any value that is not text, a blank cell included (the IS functions page)";
const IS_LOGICAL: &str = "ISLOGICAL is TRUE when value is a logical value (the IS functions page)";
const TRUNCATED: &str = "a number that is not an integer is truncated (the ISEVEN and ISODD pages)";
const PARITY_NONNUMERIC: &str =
    "a value that is not numeric is #VALUE! (the ISEVEN and ISODD pages)";
const IS_REF: &str = "ISREF is TRUE when value refers to a reference (the IS functions page)";
const NOT_REF: &str = "ISREF is FALSE for a value that is not a reference (the IS functions page)";
const TYPE_TABLE: &str = "TYPE gives 1 for a number, 2 for text, 4 for a logical, 16 for an error and 64 for an array (the TYPE page)";
const TYPE_RESULT: &str = "TYPE gives the type of a formula's resulting value (the TYPE page)";
const N_TABLE: &str = "N gives a number as itself, TRUE as 1, FALSE as 0, an error as itself and anything else as 0 (the N page)";
const NA_PAGE: &str = "NA() is the error value #N/A (the NA page)";
const ERROR_TYPE_TABLE: &str = "ERROR.TYPE gives #NULL! 1, #DIV/0! 2, #VALUE! 3, #REF! 4, #NAME? 5, #NUM! 6, #N/A 7 and #GETTING_DATA 8, and #N/A for anything else (the ERROR.TYPE page)";
const IF_FALSE: &str = "IF gives value_if_false when logical_test is FALSE (the IF page)";
const BELOW_INTEGER: &str = "a number less than about 1e-10 below an integer counts as that integer: ISEVEN and ISODD truncate the magnitude plus 1e-10, rounded once to binary64";

/// The rows of dc_nzpw__information_functions, run through calc_func::invoke on `s`, in the order
/// the information oracle workbook's sheet holds their formulas.
fn information_rows(s: &mut TestSheet, quiet: bool) -> InfoRows {
    const CASE: &str = "dc_nzpw__information_functions";
    use InfoBasis::{Documented as Doc, Measured as Meas, Refused as Ref};
    use WorksheetErrorCode as W;
    let mut info = InfoRows {
        rows: Rows::new(CASE, INFORMATION_SHEET, quiet),
        basis: Vec::new(),
        sources_read: BTreeMap::new(),
        dynamic: BTreeSet::new(),
    };
    let r = &mut info;
    let one = || CalcValue::number(1.0);
    let getting_data = || arg("#GETTING_DATA", CalcValue::error(W::GettingData));

    // ISBLANK over a blank cell, empty text held and given by a formula, other values and an array.
    info!(r, s, "ISBLANK", [cell(s, "A1")], L(true), Doc(IS_BLANK));
    info!(
        r,
        s,
        "ISBLANK",
        [cell(s, "A2")],
        L(false),
        Meas("empty text held as a constant is not blank; the page names only an empty cell")
    );
    info!(
        r,
        s,
        "IF",
        [cell(s, "A4"), txt("x"), txt("")],
        t(""),
        Doc(IF_FALSE)
    );
    let d3 = r.rows.oracle.last().expect("row 3").outcome.clone();
    assert!(
        d3.as_ref()
            .is_ok_and(|value| identical(value, &s.value("D3"))),
        "row 3 gives {d3:?}, which the sheet holds for D3"
    );
    info!(
        r,
        s,
        "ISBLANK",
        [cell(s, "D3")],
        L(false),
        Meas("a formula's empty text is not blank")
    );
    info!(
        r,
        s,
        "ISBLANK",
        [txt("")],
        L(false),
        Meas("empty text given directly is not blank")
    );
    info!(
        r,
        s,
        "ISBLANK",
        [cell(s, "A3")],
        L(false),
        Doc(IS_NOT_BLANK)
    );
    info!(
        r,
        s,
        "ISBLANK",
        [cell(s, "B7")],
        L(false),
        Doc(IS_NOT_BLANK)
    );
    info!(
        r,
        s,
        "ISBLANK",
        [array_arg(
            "{1,2}",
            vec![vec![one(), CalcValue::number(2.0)]]
        )],
        Refused(Some(0)),
        Ref(ARRAY_ARGUMENT)
    );

    // ISERR, ISERROR and ISNA over every error a cell or a formula here holds, and other values.
    for (address, code) in [
        ("B1", W::Null),
        ("B2", W::Div0),
        ("B3", W::Value),
        ("B4", W::Ref),
        ("B5", W::Name),
        ("B6", W::Num),
    ] {
        assert_eq!(s.value(address), CalcValue::error(code));
        info!(r, s, "ISERR", [cell(s, address)], L(true), Doc(IS_ERR));
    }
    info!(r, s, "ISERR", [cell(s, "B7")], L(false), Doc(IS_ERR));
    info!(
        r,
        s,
        "ISERR",
        [cell(s, "B8")],
        L(true),
        Meas("#GETTING_DATA, which the page does not name, is an error ISERR is TRUE for")
    );
    info!(
        r,
        s,
        "ISERR",
        [cell(s, "B9")],
        L(true),
        Meas("#BUSY!, held as a constant, is an error ISERR is TRUE for")
    );
    info!(r, s, "ISERR", [cell(s, "A7")], L(false), Doc(IS_ERR));
    info!(
        r,
        s,
        "ISERR",
        [cell(s, "A1")],
        L(false),
        Meas("a blank cell is no error")
    );
    info!(
        r,
        s,
        "ISERR",
        [getting_data()],
        L(true),
        Meas("the literal #GETTING_DATA is an error ISERR is TRUE for")
    );
    info!(
        r,
        s,
        "ISERR",
        [cell(s, "F1")],
        L(true),
        Meas("#CALC! is an error ISERR is TRUE for")
    );
    info!(
        r,
        s,
        "ISERR",
        [cell(s, "F2")],
        L(true),
        Meas("#SPILL! is an error ISERR is TRUE for")
    );
    info!(
        r,
        s,
        "ISERR",
        [cell(s, "F4")],
        L(true),
        Meas("#FIELD! is an error ISERR is TRUE for")
    );
    info!(r, s, "ISERROR", [cell(s, "B7")], L(true), Doc(IS_ERROR));
    info!(r, s, "ISERROR", [cell(s, "B2")], L(true), Doc(IS_ERROR));
    info!(
        r,
        s,
        "ISERROR",
        [cell(s, "B8")],
        L(true),
        Meas("#GETTING_DATA is an error ISERROR is TRUE for")
    );
    info!(
        r,
        s,
        "ISERROR",
        [cell(s, "B9")],
        L(true),
        Meas("#BUSY! is an error ISERROR is TRUE for")
    );
    info!(r, s, "ISERROR", [cell(s, "A2")], L(false), Doc(IS_ERROR));
    info!(r, s, "ISERROR", [err(W::Div0)], L(true), Doc(IS_ERROR));
    info!(
        r,
        s,
        "ISERROR",
        [cell(s, "F1")],
        L(true),
        Meas("#CALC! is an error ISERROR is TRUE for")
    );
    info!(
        r,
        s,
        "ISERROR",
        [cell(s, "F2")],
        L(true),
        Meas("#SPILL! is an error ISERROR is TRUE for")
    );
    info!(
        r,
        s,
        "ISERROR",
        [cell(s, "F4")],
        L(true),
        Meas("#FIELD! is an error ISERROR is TRUE for")
    );
    info!(r, s, "ISNA", [cell(s, "B7")], L(true), Doc(IS_NA));
    info!(r, s, "ISNA", [cell(s, "B2")], L(false), Doc(IS_NA));
    info!(
        r,
        s,
        "ISNA",
        [cell(s, "B8")],
        L(false),
        Meas("#GETTING_DATA is not #N/A")
    );
    info!(
        r,
        s,
        "ISNA",
        [cell(s, "B9")],
        L(false),
        Meas("#BUSY! is not #N/A")
    );
    info!(r, s, "ISNA", [err(W::NA)], L(true), Doc(IS_NA));
    info!(
        r,
        s,
        "ISNA",
        [cell(s, "F1")],
        L(false),
        Meas("#CALC! is not #N/A")
    );

    // ISNUMBER, ISTEXT, ISNONTEXT and ISLOGICAL, whose argument is not converted.
    info!(r, s, "ISNUMBER", [cell(s, "A3")], L(true), Doc(IS_NUMBER));
    info!(r, s, "ISNUMBER", [cell(s, "A8")], L(true), Doc(IS_NUMBER));
    info!(
        r,
        s,
        "ISNUMBER",
        [cell(s, "A6")],
        L(false),
        Doc(NOT_CONVERTED)
    );
    info!(r, s, "ISNUMBER", [txt("19")], L(false), Doc(NOT_CONVERTED));
    info!(
        r,
        s,
        "ISNUMBER",
        [cell(s, "A5")],
        L(false),
        Meas("TRUE in a cell is no number, though arithmetic reads it as 1")
    );
    info!(
        r,
        s,
        "ISNUMBER",
        [cell(s, "A1")],
        L(false),
        Meas("a blank cell is no number, though arithmetic reads it as 0")
    );
    info!(r, s, "ISTEXT", [cell(s, "A6")], L(true), Doc(IS_TEXT));
    info!(
        r,
        s,
        "ISTEXT",
        [cell(s, "A2")],
        L(true),
        Meas("empty text held as a constant is text")
    );
    info!(
        r,
        s,
        "ISTEXT",
        [cell(s, "D3")],
        L(true),
        Meas("a formula's empty text is text")
    );
    info!(
        r,
        s,
        "ISTEXT",
        [txt("")],
        L(true),
        Meas("empty text given directly is text")
    );
    info!(
        r,
        s,
        "ISTEXT",
        [cell(s, "A1")],
        L(false),
        Meas("a blank cell is not text")
    );
    info!(r, s, "ISTEXT", [cell(s, "A3")], L(false), Doc(IS_TEXT));
    info!(r, s, "ISNONTEXT", [cell(s, "A1")], L(true), Doc(IS_NONTEXT));
    info!(
        r,
        s,
        "ISNONTEXT",
        [cell(s, "A2")],
        L(false),
        Meas("empty text held as a constant is text")
    );
    info!(
        r,
        s,
        "ISNONTEXT",
        [cell(s, "D3")],
        L(false),
        Meas("a formula's empty text is text")
    );
    info!(r, s, "ISNONTEXT", [cell(s, "B7")], L(true), Doc(IS_NONTEXT));
    info!(r, s, "ISNONTEXT", [cell(s, "A5")], L(true), Doc(IS_NONTEXT));
    info!(
        r,
        s,
        "ISNONTEXT",
        [cell(s, "A7")],
        L(false),
        Doc(IS_NONTEXT)
    );
    info!(r, s, "ISLOGICAL", [cell(s, "A4")], L(true), Doc(IS_LOGICAL));
    info!(r, s, "ISLOGICAL", [logical(true)], L(true), Doc(IS_LOGICAL));
    info!(
        r,
        s,
        "ISLOGICAL",
        [cell(s, "A11")],
        L(false),
        Doc(NOT_CONVERTED)
    );
    info!(
        r,
        s,
        "ISLOGICAL",
        [cell(s, "A3")],
        L(false),
        Doc(IS_LOGICAL)
    );
    info!(
        r,
        s,
        "ISLOGICAL",
        [cell(s, "A1")],
        L(false),
        Meas("a blank cell is no logical, though IF reads it as FALSE")
    );

    // ISEVEN and ISODD: truncation, number text, logicals, errors, references of several cells or
    // areas, large integers, an array and number text read by Excel's number-text rules.
    info!(
        r,
        s,
        "ISEVEN",
        [reference(s, &["A3"])],
        L(true),
        Doc(TRUNCATED)
    );
    info!(
        r,
        s,
        "ISEVEN",
        [reference(s, &["A1"])],
        L(true),
        Meas("a blank cell reads as 0, which is even")
    );
    info!(
        r,
        s,
        "ISEVEN",
        [reference(s, &["A8"])],
        L(true),
        Doc(TRUNCATED)
    );
    info!(
        r,
        s,
        "ISODD",
        [reference(s, &["A9"])],
        L(true),
        Doc(TRUNCATED)
    );
    info!(
        r,
        s,
        "ISEVEN",
        [reference(s, &["C6"])],
        L(true),
        Meas("-2.5 is truncated toward zero, to -2")
    );
    info!(
        r,
        s,
        "ISODD",
        [num("-1.5")],
        L(true),
        Meas("-1.5 is truncated toward zero, to -1")
    );
    info!(
        r,
        s,
        "ISEVEN",
        [num("-0.5")],
        L(true),
        Meas("-0.5 is truncated toward zero, to 0")
    );
    info!(
        r,
        s,
        "ISEVEN",
        [reference(s, &["A6"])],
        L(true),
        Meas(
            "number text in a cell is read as the number, against the page's #VALUE! for a value that is not numeric"
        )
    );
    info!(
        r,
        s,
        "ISEVEN",
        [txt("2")],
        L(true),
        Meas("number text given directly is read as the number")
    );
    info!(
        r,
        s,
        "ISEVEN",
        [reference(s, &["A12"])],
        L(true),
        Meas("the number text 1e3 in a cell is read as 1000")
    );
    info!(
        r,
        s,
        "ISEVEN",
        [txt(" 2 ")],
        L(true),
        Meas("number text with spaces around it is read as the number")
    );
    info!(
        r,
        s,
        "ISEVEN",
        [reference(s, &["A7"])],
        E(W::Value),
        Doc(PARITY_NONNUMERIC)
    );
    info!(
        r,
        s,
        "ISEVEN",
        [reference(s, &["A2"])],
        E(W::Value),
        Doc(PARITY_NONNUMERIC)
    );
    info!(
        r,
        s,
        "ISODD",
        [reference(s, &["D3"])],
        E(W::Value),
        Doc(PARITY_NONNUMERIC)
    );
    info!(
        r,
        s,
        "ISEVEN",
        [reference(s, &["A5"])],
        E(W::Value),
        Meas("TRUE in a cell is #VALUE!, though arithmetic reads it as 1")
    );
    info!(
        r,
        s,
        "ISEVEN",
        [logical(true)],
        E(W::Value),
        Meas("TRUE given directly is #VALUE!")
    );
    info!(
        r,
        s,
        "ISEVEN",
        [reference(s, &["B2"])],
        E(W::Div0),
        Meas("an error in the cell is the result")
    );
    info!(
        r,
        s,
        "ISEVEN",
        [reference(s, &["F1"])],
        E(W::Calc),
        Meas("#CALC! in the cell is the result")
    );
    info!(
        r,
        s,
        "ISEVEN",
        [reference(s, &["A1:A2"])],
        E(W::Value),
        Meas("a range of several cells is #VALUE!, not intersected: Excel loads it without @")
    );
    info!(
        r,
        s,
        "ISODD",
        [reference(s, &["A3", "A8"])],
        E(W::Value),
        Meas("a reference of several areas is #VALUE!")
    );
    info!(
        r,
        s,
        "ISODD",
        [reference(s, &["C1"])],
        L(true),
        Meas("2^53-1 is odd")
    );
    info!(
        r,
        s,
        "ISEVEN",
        [reference(s, &["C2"])],
        L(true),
        Meas("the largest binary64, an integer of 2^1023 or more, is even")
    );
    info!(
        r,
        s,
        "ISODD",
        [reference(s, &["C3"])],
        L(true),
        Meas("-(2^53-1) is odd")
    );
    info!(
        r,
        s,
        "ISODD",
        [reference(s, &["C4"])],
        L(true),
        Meas("2^52+1 is odd")
    );
    info!(
        r,
        s,
        "ISODD",
        [reference(s, &["C5"])],
        L(false),
        Meas("1E+300, an integer beyond 2^53, is even")
    );
    info!(
        r,
        s,
        "ISEVEN",
        [reference(s, &["C7"])],
        E(W::Value),
        Meas(
            "the text 1e308, whose number is beyond the range Excel reads number text in, is #VALUE!"
        )
    );
    info!(
        r,
        s,
        "ISEVEN",
        [loaded_first("{2,3}", CalcValue::number(2.0))],
        L(true),
        Meas(LEGACY_FIRST)
    );
    info!(
        r,
        s,
        "ISEVEN",
        [reference(s, &["C8"])],
        Refused(Some(0)),
        Ref(NUMBER_TEXT)
    );
    info!(
        r,
        s,
        "ISEVEN",
        [txt("$4")],
        Refused(Some(0)),
        Ref(NUMBER_TEXT)
    );

    // ISREF and ISFORMULA.
    info!(r, s, "ISREF", [reference(s, &["A1"])], L(true), Doc(IS_REF));
    info!(
        r,
        s,
        "ISREF",
        [reference(s, &["A1:B2"])],
        L(true),
        Doc(IS_REF)
    );
    info!(
        r,
        s,
        "ISREF",
        [reference(s, &["A1", "B2"])],
        L(true),
        Doc(IS_REF)
    );
    info!(r, s, "ISREF", [reference(s, &["B7"])], L(true), Doc(IS_REF));
    info!(r, s, "ISREF", [num("1")], L(false), Doc(NOT_REF));
    info!(r, s, "ISREF", [txt("A1")], L(false), Doc(NOT_REF));
    info!(
        r,
        s,
        "ISREF",
        [err(W::Ref)],
        L(false),
        Meas("the error #REF! is no reference")
    );
    info!(
        r,
        s,
        "ISREF",
        [array_arg("{1}", vec![vec![one()]])],
        L(false),
        Meas("an array is no reference, and ISREF is not lifted over it: Excel loads it without @")
    );
    info!(
        r,
        s,
        "ISFORMULA",
        [reference(s, &["D3"])],
        Refused(Some(0)),
        Ref(FORMULA_FACT)
    );
    info!(
        r,
        s,
        "ISFORMULA",
        [reference(s, &["A3"])],
        Refused(Some(0)),
        Ref(FORMULA_FACT)
    );

    // TYPE.
    info!(
        r,
        s,
        "TYPE",
        [reference(s, &["A1"])],
        N(1.0),
        Meas("a blank cell is of type 1, as a number")
    );
    info!(
        r,
        s,
        "TYPE",
        [reference(s, &["A3"])],
        N(1.0),
        Doc(TYPE_TABLE)
    );
    info!(
        r,
        s,
        "TYPE",
        [reference(s, &["A2"])],
        N(2.0),
        Doc(TYPE_TABLE)
    );
    info!(
        r,
        s,
        "TYPE",
        [reference(s, &["D3"])],
        N(2.0),
        Doc(TYPE_RESULT)
    );
    info!(
        r,
        s,
        "TYPE",
        [reference(s, &["A4"])],
        N(4.0),
        Doc(TYPE_TABLE)
    );
    info!(
        r,
        s,
        "TYPE",
        [reference(s, &["B7"])],
        N(16.0),
        Doc(TYPE_TABLE)
    );
    info!(
        r,
        s,
        "TYPE",
        [reference(s, &["B9"])],
        N(16.0),
        Meas("#BUSY! is an error, of type 16")
    );
    info!(
        r,
        s,
        "TYPE",
        [array_arg(
            "{1,2}",
            vec![vec![one(), CalcValue::number(2.0)]]
        )],
        N(64.0),
        Doc(TYPE_TABLE)
    );
    info!(
        r,
        s,
        "TYPE",
        [array_arg("{1}", vec![vec![one()]])],
        N(64.0),
        Meas("an array of one element is of type 64")
    );
    info!(
        r,
        s,
        "TYPE",
        [reference(s, &["A1", "A2"])],
        N(16.0),
        Meas("a reference of several areas is of type 16: Excel loads it without @")
    );
    info!(
        r,
        s,
        "TYPE",
        [arg("A1:A2", CalcValue::error(W::Value))],
        N(16.0),
        Meas(
            "dc-b1l: the legacy formula in row 111 below A1:A2 intersects the range before the call (Excel writes TYPE(@A1:A2), calc-fml's legacy places), giving TYPE #VALUE!, of type 16; a range itself is of type 64, as the reader test below checks"
        )
    );
    info!(r, s, "TYPE", [txt("")], N(2.0), Doc(TYPE_TABLE));
    info!(r, s, "TYPE", [err(W::NA)], N(16.0), Doc(TYPE_TABLE));
    info!(
        r,
        s,
        "TYPE",
        [reference(s, &["F1"])],
        N(16.0),
        Meas("#CALC! is an error, of type 16")
    );
    info!(
        r,
        s,
        "TYPE",
        [reference(s, &["F2"])],
        N(16.0),
        Meas("#SPILL! is an error, of type 16")
    );

    // N and NA.
    info!(r, s, "N", [reference(s, &["A8"])], N(2.5), Doc(N_TABLE));
    info!(r, s, "N", [reference(s, &["A5"])], N(1.0), Doc(N_TABLE));
    info!(r, s, "N", [reference(s, &["A4"])], N(0.0), Doc(N_TABLE));
    info!(r, s, "N", [reference(s, &["A6"])], N(0.0), Doc(N_TABLE));
    info!(r, s, "N", [reference(s, &["A1"])], N(0.0), Doc(N_TABLE));
    info!(r, s, "N", [reference(s, &["A2"])], N(0.0), Doc(N_TABLE));
    info!(r, s, "N", [reference(s, &["B2"])], E(W::Div0), Doc(N_TABLE));
    info!(r, s, "N", [txt("7")], N(0.0), Doc(N_TABLE));
    info!(r, s, "N", [logical(true)], N(1.0), Doc(N_TABLE));
    info!(
        r,
        s,
        "N",
        [reference(s, &["A8:A9"])],
        N(2.5),
        Meas("a range of several cells is read at its first cell: Excel loads it without @")
    );
    info!(
        r,
        s,
        "N",
        [reference(s, &["A5", "A8"])],
        N(1.0),
        Meas("a reference of several areas is read at the first cell of its first area")
    );
    info!(r, s, "N", [reference(s, &["D3"])], N(0.0), Doc(N_TABLE));
    info!(
        r,
        s,
        "N",
        [loaded_first("{5,6}", CalcValue::number(5.0))],
        N(5.0),
        Meas(LEGACY_FIRST)
    );
    info!(
        r,
        s,
        "N",
        [reference(s, &["F1"])],
        E(W::Calc),
        Meas("#CALC! in the cell is the result")
    );
    info!(r, s, "NA", [], E(W::NA), Doc(NA_PAGE));

    // ERROR.TYPE.
    for (address, number) in [
        ("B1", 1.0),
        ("B2", 2.0),
        ("B3", 3.0),
        ("B4", 4.0),
        ("B5", 5.0),
        ("B6", 6.0),
        ("B7", 7.0),
        ("B8", 8.0),
    ] {
        info!(
            r,
            s,
            "ERROR.TYPE",
            [cell(s, address)],
            N(number),
            Doc(ERROR_TYPE_TABLE)
        );
    }
    info!(
        r,
        s,
        "ERROR.TYPE",
        [cell(s, "B9")],
        N(0.0),
        Meas("#BUSY! is 0, where the page gives #N/A for anything not listed")
    );
    info!(
        r,
        s,
        "ERROR.TYPE",
        [getting_data()],
        N(8.0),
        Doc(ERROR_TYPE_TABLE)
    );
    info!(
        r,
        s,
        "ERROR.TYPE",
        [cell(s, "F1")],
        N(14.0),
        Meas("#CALC! is 14, where the page gives #N/A for anything not listed")
    );
    info!(
        r,
        s,
        "ERROR.TYPE",
        [cell(s, "F2")],
        N(9.0),
        Meas("#SPILL! is 9, where the page gives #N/A for anything not listed")
    );
    info!(
        r,
        s,
        "ERROR.TYPE",
        [cell(s, "F4")],
        N(13.0),
        Meas("#FIELD! is 13, where the page gives #N/A for anything not listed")
    );
    info!(
        r,
        s,
        "ERROR.TYPE",
        [cell(s, "A3")],
        E(W::NA),
        Doc(ERROR_TYPE_TABLE)
    );
    info!(
        r,
        s,
        "ERROR.TYPE",
        [cell(s, "A1")],
        E(W::NA),
        Doc(ERROR_TYPE_TABLE)
    );
    info!(
        r,
        s,
        "ERROR.TYPE",
        [cell(s, "A7")],
        E(W::NA),
        Doc(ERROR_TYPE_TABLE)
    );
    info!(
        r,
        s,
        "ERROR.TYPE",
        [array_arg(
            "{1,#N/A}",
            vec![vec![one(), CalcValue::error(W::NA)]]
        )],
        Refused(Some(0)),
        Ref(ARRAY_ARGUMENT)
    );

    // A range given to the value argument of ISBLANK, ISERR, ISERROR, ISNA, ISNUMBER, ISTEXT,
    // ISNONTEXT, ISLOGICAL and ERROR.TYPE: Excel loads each with @ (the comparison checks the
    // loaded text), and the caller intersects it before the call, so calc_func::invoke refuses
    // the reference at admission.
    for (spelling, range) in [
        ("ISBLANK", "A1:A2"),
        ("ISERR", "B1:B2"),
        ("ISERROR", "B1:B2"),
        ("ISNA", "B1:B2"),
        ("ISNUMBER", "A1:A2"),
        ("ISTEXT", "A1:A2"),
        ("ISNONTEXT", "A1:A2"),
        ("ISLOGICAL", "A4:A5"),
        ("ERROR.TYPE", "B1:B2"),
    ] {
        info!(
            r,
            s,
            spelling,
            [reference(s, &[range])],
            reference_argument,
            Ref(RANGE_TO_VALUE)
        );
    }

    // B9's loaded error, as N gives it: N gives an error as itself, so Excel's result of N(B9) is
    // the error Excel loaded B9 as.
    info!(
        r,
        s,
        "N",
        [reference(s, &["B9"])],
        E(W::Busy),
        InfoBasis::Unrecorded(N_TABLE, BUSY_RAW)
    );

    // ISEVEN and ISODD just below an integer, typed and in a cell.
    info!(
        r,
        s,
        "ISEVEN",
        [num("2.99999999999")],
        L(false),
        Meas(BELOW_INTEGER)
    );
    info!(
        r,
        s,
        "ISODD",
        [num("2.99999999999")],
        L(true),
        Meas(BELOW_INTEGER)
    );
    info!(
        r,
        s,
        "ISEVEN",
        [reference(s, &["C13"])],
        L(false),
        Meas(BELOW_INTEGER)
    );
    info!(
        r,
        s,
        "ISEVEN",
        [num("-2.99999999999")],
        L(false),
        Meas(BELOW_INTEGER)
    );
    info!(
        r,
        s,
        "ISODD",
        [num("2.99999999991")],
        L(true),
        Meas("0.9e-10 below 3 counts as 3")
    );
    info!(
        r,
        s,
        "ISODD",
        [num("2.99999999989")],
        L(false),
        Meas("1.1e-10 below 3 is truncated to 2")
    );
    info!(
        r,
        s,
        "ISODD",
        [reference(s, &["C11"])],
        L(true),
        Meas(
            "two binary64 steps (1.16e-10) below 300001 count as 300001: the magnitude plus 1e-10 rounds to 300001"
        )
    );
    info!(
        r,
        s,
        "ISODD",
        [reference(s, &["C12"])],
        L(false),
        Meas(
            "three binary64 steps (1.75e-10) below 300001 are truncated to 300000: the magnitude plus 1e-10 rounds below 300001"
        )
    );
    for address in ["C11", "C12"] {
        let held = s.value(address);
        let number = held.as_number().expect("a number");
        info!(
            r,
            s,
            "N",
            [reference(s, &[address])],
            N(number),
            Doc(N_TABLE)
        );
    }

    // ISEVEN, ISODD and N over a range of several cells, each placed within its range's rows, so
    // intersection would read the cell of its own row: as legacy formulas, then as dynamic-array
    // formulas.
    let row = info!(
        r,
        s,
        "ISEVEN",
        [reference(s, &["E167:E169"])],
        E(W::Value),
        Meas(
            "a range of several cells is #VALUE! in a legacy formula within the range's rows: not intersected (E168, in the formula's row, holds 2)"
        )
    );
    assert_eq!(
        row, 168,
        "the rows over E167:E174 sit within their ranges' rows"
    );
    info!(
        r,
        s,
        "ISODD",
        [reference(s, &["E168:E170"])],
        E(W::Value),
        Meas(
            "a range of several cells is #VALUE! in a legacy formula within the range's rows: not intersected (E169, in the formula's row, holds 5)"
        )
    );
    info!(
        r,
        s,
        "N",
        [reference(s, &["E169:E171"])],
        N(5.0),
        Meas(
            "a range of several cells is read at its first cell, E169, in a legacy formula within the range's rows, not at E170 in the formula's row"
        )
    );
    let first_dynamic = info!(
        r,
        s,
        "ISEVEN",
        [reference(s, &["E170:E172"])],
        E(W::Value),
        Meas(
            "a range of several cells is #VALUE! in a dynamic-array formula: ISEVEN is not lifted over its cells"
        )
    );
    r.dynamic.insert(first_dynamic);
    let row = info!(
        r,
        s,
        "ISODD",
        [reference(s, &["E171:E173"])],
        E(W::Value),
        Meas(
            "a range of several cells is #VALUE! in a dynamic-array formula: ISODD is not lifted over its cells"
        )
    );
    r.dynamic.insert(row);
    let row = info!(
        r,
        s,
        "N",
        [reference(s, &["E172:E174"])],
        N(6.0),
        Meas(
            "a range of several cells is read at its first cell, E172, in a dynamic-array formula: N is not lifted over its cells"
        )
    );
    r.dynamic.insert(row);
    assert_eq!(
        r.dynamic.iter().copied().collect::<Vec<_>>(),
        [171, 172, 173],
        "the dynamic-array rows the generator writes"
    );

    // ISNA over a blank cell, and ISNA, ISNUMBER, ISLOGICAL and ISERR over empty text held as a
    // constant and given by a formula; then N of C13, the cell row 160 reads, as Excel holds it.
    info!(
        r,
        s,
        "ISNA",
        [cell(s, "A1")],
        L(false),
        Meas("a blank cell is not #N/A")
    );
    for (spelling, held, given) in [
        (
            "ISNA",
            "empty text held as a constant is not #N/A",
            "a formula's empty text is not #N/A",
        ),
        (
            "ISNUMBER",
            "empty text held as a constant is no number",
            "a formula's empty text is no number",
        ),
        (
            "ISLOGICAL",
            "empty text held as a constant is no logical",
            "a formula's empty text is no logical",
        ),
        (
            "ISERR",
            "empty text held as a constant is no error",
            "a formula's empty text is no error",
        ),
    ] {
        info!(r, s, spelling, [cell(s, "A2")], L(false), Meas(held));
        info!(r, s, spelling, [cell(s, "D3")], L(false), Meas(given));
    }
    let c13 = s.value("C13").as_number().expect("a number");
    info!(r, s, "N", [reference(s, &["C13"])], N(c13), Doc(N_TABLE));
    info
}

/// The pairs `(family 10 member, spelling)` the catalogue holds.
fn information_family() -> BTreeSet<(u32, String)> {
    catalogue()
        .targets()
        .filter(|target| target.id().raw() >> 16 == 10)
        .map(|target| (target.id().raw(), target.spelling().to_owned()))
        .collect()
}

/// One call of `spelling` on `values` through calc_func::invoke, with `cells` as the reader when
/// given.
fn information_call(
    spelling: &str,
    values: Vec<CalcValue>,
    mode: &NumericMode,
    cells: Option<&mut dyn CellReader>,
) -> Result<CalcValue, FunctionFault> {
    let mut services = match cells {
        Some(cells) => EvalServices::with_cells(mode, cells),
        None => EvalServices::new(mode),
    };
    invoke(
        surface(Notation::Call, spelling),
        &mut Recorded::new(values),
        &mut services,
        &mut FunctionCallScratch::new(),
    )
}

/// A formula as the information oracle workbook stores it: ISFORMULA and the source formulas'
/// functions carry their `_xlfn.` prefixes (FILTER also `_xlws.`).
fn information_stored(formula: &str) -> String {
    formula
        .replace("ISFORMULA(", "_xlfn.ISFORMULA(")
        .replace("FILTER(", "_xlfn._xlws.FILTER(")
        .replace("SEQUENCE(", "_xlfn.SEQUENCE(")
        .replace("FIELDVALUE(", "_xlfn.FIELDVALUE(")
}

/// The rows whose formula Excel loads with another text than the row's: each formula as the row
/// writes it and as Excel 16.0.20430.20182 loaded it in bead dc-nzpw's capture. Excel inserts `@`
/// before an array or a range of one area given to a value argument in a legacy formula, and not
/// before a range given to ISEVEN, ISODD or N, a reference of several areas, or an array given to
/// TYPE or ISREF (rows that keep their text).
const INFORMATION_LOADED: [(&str, &str); 14] = [
    ("=ISBLANK({1,2})", "=ISBLANK(@{1,2})"),
    ("=ISEVEN({2,3})", "=ISEVEN(@{2,3})"),
    ("=TYPE(A1:A2)", "=TYPE(@A1:A2)"),
    ("=N({5,6})", "=N(@{5,6})"),
    ("=ERROR.TYPE({1,#N/A})", "=ERROR.TYPE(@{1,#N/A})"),
    ("=ISBLANK(A1:A2)", "=ISBLANK(@A1:A2)"),
    ("=ISERR(B1:B2)", "=ISERR(@B1:B2)"),
    ("=ISERROR(B1:B2)", "=ISERROR(@B1:B2)"),
    ("=ISNA(B1:B2)", "=ISNA(@B1:B2)"),
    ("=ISNUMBER(A1:A2)", "=ISNUMBER(@A1:A2)"),
    ("=ISTEXT(A1:A2)", "=ISTEXT(@A1:A2)"),
    ("=ISNONTEXT(A1:A2)", "=ISNONTEXT(@A1:A2)"),
    ("=ISLOGICAL(A4:A5)", "=ISLOGICAL(@A4:A5)"),
    ("=ERROR.TYPE(B1:B2)", "=ERROR.TYPE(@B1:B2)"),
];

/// A row's formula as Excel loads it.
fn information_loaded(formula: &str) -> String {
    INFORMATION_LOADED
        .iter()
        .find(|(written, _)| *written == formula)
        .map_or(formula, |(_, loaded)| loaded)
        .to_owned()
}

/// The saved cache Excel writes for a value: an error as its literal, anything else as text.
fn information_saved_cache(value: &CalcValue) -> SavedCache {
    let cache = match value.core() {
        CoreValue::Error(code) => SavedCache::new(
            CacheCellType::Error,
            ALL_ERROR_LITERALS
                .iter()
                .find(|(known, _)| known == code)
                .map(|(_, literal)| *literal)
                .expect("every error has a literal"),
        ),
        _ => SavedCache::new(CacheCellType::Text, &shown_value(value)),
    };
    cache.expect("a saved cache")
}

/// The error constants the information oracle workbook holds, by their literals.
const INFORMATION_ERROR_LITERALS: [(WorksheetErrorCode, &str); 9] = [
    (WorksheetErrorCode::Null, "#NULL!"),
    (WorksheetErrorCode::Div0, "#DIV/0!"),
    (WorksheetErrorCode::Value, "#VALUE!"),
    (WorksheetErrorCode::Ref, "#REF!"),
    (WorksheetErrorCode::Name, "#NAME?"),
    (WorksheetErrorCode::Num, "#NUM!"),
    (WorksheetErrorCode::NA, "#N/A"),
    (WorksheetErrorCode::GettingData, "#GETTING_DATA"),
    (WorksheetErrorCode::Busy, "#BUSY!"),
];

/// A cell of the information oracle sheet: a formula (its text, without `=`, and whether it is a
/// dynamic-array formula over its one cell) or a constant.
enum InfoCell {
    Formula { text: String, dynamic: bool },
    Constant(CalcValue),
}

/// The cells of the information oracle sheet's part.
fn information_cells(xml: &str) -> BTreeMap<(u32, u32), InfoCell> {
    let mut cells = BTreeMap::new();
    let mut rest = xml;
    while let Some(at) = rest.find("<c ") {
        rest = &rest[at..];
        let tag_end = rest.find('>').expect("a cell's start tag ends");
        let tag = &rest[..tag_end];
        let close = rest.find("</c>").expect("a cell ends");
        let body = &rest[tag_end + 1..close];
        rest = &rest[close + 4..];
        let reference = attribute(tag, "r").expect("a cell names its reference");
        let inner = |name: &str| -> Option<String> {
            let open = format!("<{name}>");
            let start = body.find(&open)? + open.len();
            let end = body[start..].find(&format!("</{name}>"))? + start;
            Some(unescape(&body[start..end]))
        };
        let cell = if let Some(at) = body.find("<f") {
            let f_tag_end = at + body[at..].find('>').expect("a formula's start tag ends");
            let f_tag = &body[at..f_tag_end];
            let end = body.find("</f>").expect("a formula ends");
            let dynamic = attribute(tag, "cm") == Some("1")
                && attribute(f_tag, "t") == Some("array")
                && attribute(f_tag, "ref") == Some(reference);
            assert!(
                dynamic || f_tag == "<f",
                "{reference}: a formula of another kind: {f_tag}"
            );
            InfoCell::Formula {
                text: unescape(&body[f_tag_end + 1..end]),
                dynamic,
            }
        } else {
            let value =
                |name: &str| inner(name).unwrap_or_else(|| panic!("{reference} has no <{name}>"));
            InfoCell::Constant(match attribute(tag, "t") {
                None | Some("n") => CalcValue::number(value("v").parse().expect("a number")),
                Some("b") => CalcValue::logical(value("v") == "1"),
                Some("e") => {
                    let literal = value("v");
                    let code = INFORMATION_ERROR_LITERALS
                        .iter()
                        .find(|(_, known)| *known == literal)
                        .unwrap_or_else(|| {
                            panic!("{reference}: {literal} is no error literal here")
                        })
                        .0;
                    CalcValue::error(code)
                }
                Some("inlineStr") => text_value(&value("t")),
                Some(other) => panic!("{reference}: a cell of type {other}"),
            })
        };
        assert!(
            cells.insert(decode_a1(reference), cell).is_none(),
            "{reference} repeats"
        );
    }
    cells
}

/// Asserts that the information oracle workbook's sheet holds exactly the rows' formulas in column
/// D (the dynamic-array rows as dynamic-array formulas, every other row as a legacy formula), the
/// source formulas of column F as dynamic-array formulas, and the test sheet's other cells as
/// constants; that the sheet's value of D3 is row 3's result; and that its value of each source
/// cell is the error assumed for it.
fn assert_information_workbook(info: &InfoRows, sheet: &TestSheet) {
    let rows = &info.rows;
    let parts = stored_parts(INFORMATION_WORKBOOK);
    let workbook = part_text(&parts, "xl/workbook.xml");
    let relationships = part_text(&parts, "xl/_rels/workbook.xml.rels");
    let id = start_tags(workbook, "sheet")
        .into_iter()
        .find(|tag| attribute(tag, "name") == Some(rows.sheet))
        .and_then(|tag| attribute(tag, "r:id"))
        .unwrap_or_else(|| panic!("the information workbook lists no sheet {}", rows.sheet));
    let target = start_tags(relationships, "Relationship")
        .into_iter()
        .find(|tag| attribute(tag, "Id") == Some(id))
        .and_then(|tag| attribute(tag, "Target"))
        .unwrap_or_else(|| panic!("the information workbook has no relationship {id}"));
    assert!(
        part_text(&parts, "xl/metadata.xml").contains("dynamicArrayProperties fDynamic=\"1\""),
        "the information workbook's metadata part marks no dynamic-array formula"
    );
    let cells = information_cells(part_text(&parts, &format!("xl/{target}")));

    let mut expected: BTreeMap<(u32, u32), (String, bool)> = rows
        .oracle
        .iter()
        .map(|row| {
            (
                (row.row, FORMULA_COLUMN),
                (
                    information_stored(&row.formula[1..]),
                    info.dynamic.contains(&row.row),
                ),
            )
        })
        .collect();
    for source in &INFO_SOURCES {
        expected.insert(
            (source.row, SOURCE_COLUMN),
            (information_stored(&source.formula[1..]), true),
        );
        assert_eq!(decode_a1(source.address), (source.row, SOURCE_COLUMN));
        assert!(
            identical(
                &sheet.value(source.address),
                &CalcValue::error(source.assumed)
            ),
            "the sheet holds the assumed error for {}",
            source.address
        );
    }
    let at = |(row, column): (u32, u32)| format!("{}!{}", rows.sheet, encode_a1(row, column));
    let mut differences = Vec::new();
    for (place, (formula, dynamic)) in &expected {
        match cells.get(place) {
            Some(InfoCell::Formula { text, dynamic: found }) if text == formula && found == dynamic => {}
            Some(InfoCell::Formula { text, dynamic: found }) => differences.push(format!(
                "{}: the formula is {formula:?} (dynamic {dynamic}), the workbook holds {text:?} (dynamic {found})",
                at(*place)
            )),
            Some(InfoCell::Constant(value)) => differences.push(format!(
                "{}: the formula is {formula:?}, the workbook holds the constant {value:?}",
                at(*place)
            )),
            None => differences.push(format!("{}: the formula {formula:?} is absent", at(*place))),
        }
    }
    for (place, cell) in &cells {
        let held = sheet.cells.get(place);
        match cell {
            InfoCell::Formula { .. } if expected.contains_key(place) => {}
            InfoCell::Formula { text, .. } => differences.push(format!(
                "{}: a formula that no row or source has: {text:?}",
                at(*place)
            )),
            InfoCell::Constant(value) => match held {
                Some(found) if identical(found, value) => {}
                found => differences.push(format!(
                    "{}: the workbook holds {value:?}, the test sheet {found:?}",
                    at(*place)
                )),
            },
        }
    }
    for place in sheet.cells.keys() {
        let is_formula = expected.contains_key(place);
        if !cells.contains_key(place) {
            differences.push(format!(
                "{}: a cell the test sheet holds and the workbook does not",
                at(*place)
            ));
        } else if is_formula && place.1 == FORMULA_COLUMN {
            let row = rows
                .oracle
                .iter()
                .find(|row| row.row == place.0)
                .expect("a row's cell");
            match &row.outcome {
                Ok(value) if identical(value, &sheet.cells[place]) => {}
                outcome => differences.push(format!(
                    "{}: the test sheet holds {:?}, the row gives {outcome:?}",
                    at(*place),
                    sheet.cells[place]
                )),
            }
        }
    }
    assert!(
        differences.is_empty(),
        "the information workbook's sheet {} differs from the case's rows and cells:\n{}",
        rows.sheet,
        differences.join("\n")
    );
    if !rows.quiet {
        note!(
            "ACCEPT-NOTE: {}: {INFORMATION_WORKBOOK_PATH} (sha256 {}): sheet {} holds exactly the {} oracle rows' formulas in column D in row order (ISFORMULA as _xlfn.ISFORMULA; rows {:?} as dynamic-array formulas, the others as legacy formulas), the {} source formulas of column F as dynamic-array formulas (cell metadata naming XLDAPR) and the {} constant cells the rows read; the test sheet's D3 is row 3's result and its F1, F2 and F4 the errors assumed for the sources",
            rows.case,
            sha256_hex(INFORMATION_WORKBOOK),
            rows.sheet,
            rows.oracle.len(),
            info.dynamic,
            INFO_SOURCES.len(),
            cells
                .values()
                .filter(|cell| matches!(cell, InfoCell::Constant(_)))
                .count()
        );
    }
}

/// Checks that the committed information oracle workbook is its generator's output, on the route
/// where the generator runs.
fn assert_information_workbook_generated(case: &str) {
    #[cfg(all(windows, not(target_arch = "wasm32")))]
    {
        let generated = generated_fixtures();
        let names: Vec<&str> = generated.keys().map(String::as_str).collect();
        assert_eq!(
            names, GENERATED_WORKBOOKS,
            "the generator writes the oracle workbooks and nothing else"
        );
        let regenerated = &generated["dc_nzpw/oracle.xlsx"];
        assert!(
            regenerated.as_slice() == INFORMATION_WORKBOOK,
            "the committed information oracle workbook is not the generator's output (sha256 {} committed, {} generated)",
            sha256_hex(INFORMATION_WORKBOOK),
            sha256_hex(regenerated)
        );
        note!(
            "ACCEPT-NOTE: {case}: crates/calc-func/tests/fixtures/make-fixtures.ps1, run now under Windows PowerShell, wrote the committed information oracle workbook byte for byte ({} bytes)",
            regenerated.len()
        );
    }
    #[cfg(not(all(windows, not(target_arch = "wasm32"))))]
    {
        note!(
            "ACCEPT-NOTE: {case}: the generator runs under Windows PowerShell, so the check that the committed information oracle workbook is its output runs on the windows route; here the workbook is checked against the rows only"
        );
    }
}

/// A callable's handle, for the refusal of a callable argument.
#[derive(Debug)]
struct InformationCallable;

impl OpaqueCallable for InformationCallable {
    fn as_any(&self) -> &dyn Any {
        self
    }
}

#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test)]
fn dc_nzpw__information_functions() {
    const CASE: &str = "dc_nzpw__information_functions";
    use WorksheetErrorCode as W;
    let fast = NumericMode::Fast;
    let mut sheet = information_sheet();
    note!(
        "ACCEPT-NOTE: {CASE}: the sheet holds {}; A1 and A10 are blank",
        sheet.described()
    );
    let info = information_rows(&mut sheet, false);

    // Metadata and dispatch come from the one generated catalogue: family 10 holds exactly these
    // entries, each eager, with Excel's arity, its parameter classes and its signature help.
    let expected: BTreeSet<(u32, String)> = INFORMATION
        .iter()
        .map(|(spelling, member)| ((10 << 16) | member, (*spelling).to_owned()))
        .collect();
    assert_eq!(
        information_family(),
        expected,
        "the information family in the catalogue"
    );
    let value = Some(ParameterClass::Value);
    let either = Some(ParameterClass::ValueOrReference);
    let reference_only = Some(ParameterClass::Reference);
    for (spelling, class, help) in [
        ("ISBLANK", value, "ISBLANK(value)"),
        ("ISERR", value, "ISERR(value)"),
        ("ISERROR", value, "ISERROR(value)"),
        ("ISNA", value, "ISNA(value)"),
        ("ISNUMBER", value, "ISNUMBER(value)"),
        ("ISTEXT", value, "ISTEXT(value)"),
        ("ISNONTEXT", value, "ISNONTEXT(value)"),
        ("ISLOGICAL", value, "ISLOGICAL(value)"),
        ("ISEVEN", either, "ISEVEN(number)"),
        ("ISODD", either, "ISODD(number)"),
        ("ISREF", either, "ISREF(value)"),
        ("ISFORMULA", reference_only, "ISFORMULA(reference)"),
        ("TYPE", either, "TYPE(value)"),
        ("N", either, "N(value)"),
        ("ERROR.TYPE", value, "ERROR.TYPE(error_val)"),
    ] {
        let target = surface(Notation::Call, spelling);
        assert_eq!(
            target.argument_demand(),
            ArgumentDemand::Eager,
            "{spelling}"
        );
        assert_eq!(
            (target.arity().min, target.arity().max),
            (1, 1),
            "{spelling}"
        );
        assert_eq!(target.parameter_class(0), class, "{spelling}");
        assert_eq!(target.signature_help().display, help);
    }
    let na = surface(Notation::Call, "NA");
    assert_eq!((na.arity().min, na.arity().max), (0, 0));
    assert_eq!(na.parameter_class(0), None);
    assert_eq!(na.signature_help().display, "NA()");
    assert_eq!(na.oracle_formula(&[]).as_deref(), Some("=NA()"));
    assert_eq!(
        information_call("NA", vec![one_value()], &fast, None),
        Err(FunctionFault::ArityMismatch {
            id: na.id(),
            supplied: 1
        })
    );

    // Admission: a value parameter refuses a reference (the caller dereferences one cell first),
    // and ISFORMULA's reference parameter refuses anything else, as Excel refuses such a formula
    // at entry.
    let a1_key = sheet.key("A1");
    let a1 = || CalcValue::reference(ReferenceLike::cell(a1_key));
    for spelling in [
        "ISBLANK",
        "ISERR",
        "ISERROR",
        "ISNA",
        "ISNUMBER",
        "ISTEXT",
        "ISNONTEXT",
        "ISLOGICAL",
        "ERROR.TYPE",
    ] {
        assert_eq!(
            information_call(spelling, vec![a1()], &fast, Some(&mut sheet)),
            Err(FunctionFault::ReferenceArgument {
                id: surface(Notation::Call, spelling).id(),
                index: 0
            }),
            "{spelling}"
        );
    }
    let isformula = surface(Notation::Call, "ISFORMULA").id();
    for refused in [one_value(), text_value("A1"), CalcValue::error(W::NA)] {
        assert_eq!(
            information_call("ISFORMULA", vec![refused], &fast, Some(&mut sheet)),
            Err(FunctionFault::ReferenceRequired {
                id: isformula,
                index: 0
            })
        );
    }

    // The predicates over every error code: ISERROR is TRUE for each, ISERR for each but #N/A,
    // ISNA for #N/A alone, ISBLANK for none, TYPE gives 16 for each, N the error and ERROR.TYPE a
    // number. #BLOCKED! and #CONNECT!, which no capture measures (Excel refuses either as a
    // literal), are read like every other error (dc-ip6), given directly and, for TYPE and N,
    // which read a reference, from a cell; ERROR.TYPE gives them 11 and 10.
    let mut unknown_cells = TestSheet::new(&[
        ("B1", "#BLOCKED!", CalcValue::error(W::Blocked)),
        ("B2", "#CONNECT!", CalcValue::error(W::Connect)),
    ]);
    for code in W::all() {
        let error = CalcValue::error(code);
        let call = |spelling: &str| information_call(spelling, vec![error.clone()], &fast, None);
        assert_eq!(call("ISERROR"), Ok(CalcValue::logical(true)), "{code:?}");
        assert_eq!(
            call("ISERR"),
            Ok(CalcValue::logical(code != W::NA)),
            "{code:?}"
        );
        assert_eq!(
            call("ISNA"),
            Ok(CalcValue::logical(code == W::NA)),
            "{code:?}"
        );
        assert_eq!(call("ISBLANK"), Ok(CalcValue::logical(false)), "{code:?}");
        assert_eq!(call("TYPE"), Ok(CalcValue::number(16.0)), "{code:?}");
        assert_eq!(call("N"), Ok(error.clone()), "{code:?}");
        let typed = call("ERROR.TYPE");
        assert!(
            typed
                .as_ref()
                .is_ok_and(|value| value.as_number().is_some()),
            "{code:?}: {typed:?}"
        );
    }
    assert_eq!(
        information_call(
            "ERROR.TYPE",
            vec![CalcValue::error(W::Blocked)],
            &fast,
            None
        ),
        Ok(CalcValue::number(11.0))
    );
    assert_eq!(
        information_call(
            "ERROR.TYPE",
            vec![CalcValue::error(W::Connect)],
            &fast,
            None
        ),
        Ok(CalcValue::number(10.0))
    );
    for (address, code) in [("B1", W::Blocked), ("B2", W::Connect)] {
        let given = CalcValue::reference(ReferenceLike::cell(unknown_cells.key(address)));
        assert_eq!(
            information_call("TYPE", vec![given.clone()], &fast, Some(&mut unknown_cells)),
            Ok(CalcValue::number(16.0)),
            "TYPE of {address}, holding {code:?}"
        );
        assert_eq!(
            information_call("N", vec![given], &fast, Some(&mut unknown_cells)),
            Ok(CalcValue::error(code)),
            "N of {address}, holding {code:?}"
        );
    }

    // A callable, an omitted argument and a number that is not finite are refused by every entry
    // that reads a value; ISREF reads no value of a reference, so it needs no cell reader.
    let callable = CalcValue::callable(CallableValue::new(
        CallableArityShape::exact(1),
        "LAMBDA(x,x)",
        Arc::new(InformationCallable),
    ));
    for spelling in [
        "ISBLANK",
        "ISERR",
        "ISERROR",
        "ISNA",
        "ISNUMBER",
        "ISTEXT",
        "ISNONTEXT",
        "ISLOGICAL",
        "ISEVEN",
        "ISODD",
        "ISREF",
        "TYPE",
        "N",
        "ERROR.TYPE",
    ] {
        let id = surface(Notation::Call, spelling).id();
        for refused in [
            callable.clone(),
            CalcValue::missing(),
            CalcValue::number(f64::NAN),
            CalcValue::number(f64::INFINITY),
        ] {
            assert_eq!(
                information_call(spelling, vec![refused.clone()], &fast, None),
                Err(FunctionFault::NotImplemented {
                    id,
                    argument: Some(0)
                }),
                "{spelling} of {refused:?}"
            );
        }
    }
    assert_eq!(
        information_call("ISREF", vec![a1()], &fast, None),
        Ok(CalcValue::logical(true))
    );
    for spelling in ["ISEVEN", "ISODD", "TYPE", "N"] {
        assert_eq!(
            information_call(spelling, vec![a1()], &fast, None),
            Err(FunctionFault::ReferenceUnreadable {
                id: surface(Notation::Call, spelling).id(),
                index: 0
            }),
            "{spelling} without a cell reader"
        );
    }

    // dc-b1l: a reference to one area of several cells is of type 64, as Excel 16.0.20527 gives
    // =TYPE(A101:A103) and =TYPE(A101:B101) in a dynamic-array formula and
    // =SUMPRODUCT(TYPE(A101:A103)) in a legacy one (a legacy formula intersects it elsewhere).
    for area in ["A1:A2", "A3:B3"] {
        let given = CalcValue::reference(
            ReferenceLike::from_areas(vec![sheet.area(area)]).expect("one area"),
        );
        assert_eq!(
            information_call("TYPE", vec![given], &fast, Some(&mut DenseReader(&sheet))),
            Ok(CalcValue::number(64.0)),
            "TYPE({area})"
        );
    }

    // A reader that also visits blank cells, as Empty, gives what the sparse reader gives.
    for (spelling, address, expected) in [
        ("ISEVEN", "A1", CalcValue::logical(true)),
        ("TYPE", "A1", CalcValue::number(1.0)),
        ("N", "A1", CalcValue::number(0.0)),
        ("N", "A10", CalcValue::number(0.0)),
    ] {
        let given = CalcValue::reference(ReferenceLike::cell(sheet.key(address)));
        assert_eq!(
            information_call(spelling, vec![given], &fast, Some(&mut DenseReader(&sheet))),
            Ok(expected),
            "{spelling}({address}) through a reader that visits blank cells"
        );
    }

    // Under Exact, only a site that rounds is refused: ISEVEN converts number text to the nearest
    // binary64 and adds Excel's tolerance to a number's magnitude, both of which round, while the
    // IS predicates do no arithmetic.
    let exact = exact_mode();
    let iseven = surface(Notation::Call, "ISEVEN").id();
    assert_eq!(
        information_call("ISEVEN", vec![text_value("2")], &exact, None),
        Err(FunctionFault::ExactUnavailable { id: iseven })
    );
    assert_eq!(
        information_call("ISEVEN", vec![CalcValue::number(2.5)], &exact, None),
        Err(FunctionFault::ExactUnavailable { id: iseven })
    );
    assert_eq!(
        information_call("ISEVEN", vec![CalcValue::logical(true)], &exact, None),
        Ok(CalcValue::error(W::Value))
    );
    assert_eq!(
        information_call("ISBLANK", vec![CalcValue::empty()], &exact, None),
        Ok(CalcValue::logical(true))
    );
    note!(
        "ACCEPT-NOTE: {CASE}: family 10 holds exactly {} entries, each eager with its arity, parameter class and signature help; NA takes no argument; a value parameter refuses a reference and ISFORMULA anything but one at admission; ISERROR, ISERR, ISNA, ISBLANK, TYPE, N and ERROR.TYPE over all {} error codes, #BLOCKED! and #CONNECT! read as other errors (each given directly, and TYPE and N of a cell holding it); a callable, an omitted argument and NaN or infinity refused by the 14 entries that read a value; without a cell reader ISREF of a reference computes and ISEVEN, ISODD, TYPE and N refuse; a reader that visits blank cells gives the same results; under Exact ISEVEN of number text and of 2.5 is refused at its rounding sites, ISEVEN(TRUE) is #VALUE! before any, and ISBLANK computes",
        INFORMATION.len(),
        W::all().len()
    );

    assert_information_workbook(&info, &sheet);
    assert_information_workbook_generated(CASE);
    let rows = &info.rows;
    let mut owners: BTreeMap<&str, usize> = BTreeMap::new();
    for row in &rows.oracle {
        if let Some(owner) = row.owner {
            *owners.entry(owner.bead).or_default() += 1;
        }
    }
    let owners: Vec<String> = owners.iter().map(|(b, c)| format!("{c} {b}")).collect();
    note!(
        "ACCEPT-NOTE: {CASE}: {} rows through calc_func::invoke ({} of them information-function rows; row 3 is IF), each asserted against its expectation and each with its formula on sheet {INFORMATION_SHEET} of {INFORMATION_WORKBOOK_PATH} (rows {:?} as dynamic-array formulas): {} from documented rules, {} measured, {} refused and {} computed but unrecorded in the capture's value model (not modelled, owned by: {}); {} rows read a source cell of column F and are conditional on its value; dc_nzpw__excel_comparison compares every row with Excel's capture",
        rows.oracle.len(),
        rows.oracle
            .iter()
            .filter(|row| !row.formula.starts_with("=IF("))
            .count(),
        info.dynamic,
        info.count(|basis| matches!(basis, InfoBasis::Documented(_))),
        info.count(|basis| matches!(basis, InfoBasis::Measured(_))),
        info.count(|basis| matches!(basis, InfoBasis::Refused(_))),
        info.count(|basis| matches!(basis, InfoBasis::Unrecorded(..))),
        owners.join(", "),
        info.sources_read.len()
    );
}

fn one_value() -> CalcValue {
    CalcValue::number(1.0)
}

// ---------------------------------------------------------------------------------------------
// dc_nzpw__excel_comparison

/// What the information rows publish: a result for each row's cell (calculated at its call's tick
/// for a row that reads no source and computes; not modelled with its owner for a refused row; not
/// modelled, conditional on Excel's value of its source, for a row that reads a source cell), a
/// not-modelled result for each source cell, and the evaluation record of the rows' invoke calls.
fn information_documents(info: &InfoRows) -> (ResultSet, EvaluationRecord) {
    let source = format!("sha256:{}", sha256_hex(INFORMATION_WORKBOOK));
    let context = package_context(INFORMATION_WORKBOOK).fingerprint();
    let rows = &info.rows;
    let mut results = Vec::new();
    for row in &rows.oracle {
        let read = info.sources_read.get(&row.row).map(|address| {
            INFO_SOURCES
                .iter()
                .find(|source| source.address == *address)
                .expect("a source")
        });
        let (provenance, value) = match (&row.outcome, row.owner, read) {
            (Ok(value), None, None) => (
                Provenance::Calculated {
                    tick: row.tick,
                    revision: AUTHORED_REVISION,
                    context: context.clone(),
                    external_links: false,
                },
                Some(value.clone()),
            ),
            (Ok(value), None, Some(source)) => (
                Provenance::NotModeled {
                    feature: format!("reads {}, {}", source.address, source.owner.feature()),
                    diagnostic: format!(
                        "conditional on Excel's value of {}, whose formula {} calc_func has no entry for: with the sheet holding {} for it, calc_func::invoke gives {}",
                        source.address,
                        source.formula,
                        shown_value(&CalcValue::error(source.assumed)),
                        shown_value(value)
                    ),
                },
                None,
            ),
            (outcome, Some(owner), _) => (
                Provenance::NotModeled {
                    feature: owner.feature(),
                    diagnostic: match outcome {
                        Ok(value) => format!("calc_func::invoke gives {}", shown_value(value)),
                        Err(fault) => format!("calc_func::invoke refuses: {fault:?}"),
                    },
                },
                None,
            ),
            (Err(fault), None, _) => (
                Provenance::Failed {
                    fault: format!("{fault:?}"),
                },
                None,
            ),
        };
        results.push(CellResult {
            cell: oracle_cell(rows.sheet, row.row),
            provenance,
            value,
        });
    }
    for source in &INFO_SOURCES {
        results.push(CellResult {
            cell: source_cell(source),
            provenance: Provenance::NotModeled {
                feature: source.owner.feature(),
                diagnostic: format!(
                    "calc_func has no entry for the function of {}",
                    source.formula
                ),
            },
            value: None,
        });
    }
    let evaluations = rows
        .calls
        .iter()
        .map(|call| Evaluation {
            cell: oracle_cell(rows.sheet, call.row),
            tick: call.tick,
            revision: AUTHORED_REVISION,
            context: context.clone(),
        })
        .collect();
    (
        ResultSet {
            case: INFORMATION_CAPTURE_CASE.to_owned(),
            source: source.clone(),
            run: RunIdentity {
                revision: AUTHORED_REVISION,
                context,
            },
            results,
        },
        EvaluationRecord {
            case: INFORMATION_CAPTURE_CASE.to_owned(),
            source,
            evaluations,
        },
    )
}

fn source_cell(source: &InfoSource) -> CellAddress {
    CellAddress::new(INFORMATION_SHEET, source.row, SOURCE_COLUMN).expect("a cell of the grid")
}

/// The verdict on `capture` of what the rows publish, and what on it disagrees with the rows: a
/// formula cell no row or source has, a source cell whose verdict is not `not_modeled` or whose
/// raw value is not the error assumed for it, a row that reads no source and computes whose
/// verdict is not `matched`, a conditional row whose verdict is not `not_modeled` or whose raw
/// value is not the value calc_func::invoke gave, a refused or unrecorded row that names dc-nzpw
/// or whose verdict is not `not_modeled` with its owner, an unrecorded row whose cell has a raw
/// value, lacks the value failure naming #BUSY!'s code or has a saved cache other than the value
/// calc_func::invoke gave, a row or source whose loaded formula text or dynamic-array flag is not
/// the one it expects, and a row or source whose cell the inventory lacks. Returns the report, the
/// problems and how many conditional rows agree.
fn information_comparison(
    capture: &CaptureRecord,
    info: &InfoRows,
) -> Result<(VerdictReport, Vec<String>, usize), JudgeError> {
    let (results, evaluations) = information_documents(info);
    let policy = VerdictPolicy {
        numeric_mode: NumericMode::Fast,
        evidence: None,
    };
    let report = judge(capture, &results, &evaluations, &policy)?;
    let rows: BTreeMap<u32, &OracleRow> =
        info.rows.oracle.iter().map(|row| (row.row, row)).collect();
    let raw = |address: &CellAddress| -> Option<&CalcValue> {
        capture
            .observations
            .iter()
            .find_map(|observed| match &observed.observation {
                Observation::ExcelRaw(value) if observed.cell == *address => Some(value),
                _ => None,
            })
    };
    let mut problems = Vec::new();
    let mut conditional_agreed = 0;
    for cell in &report.cells {
        let address = &cell.formula.cell;
        let at = format!("{}!{}", address.sheet(), address.a1());
        if address.sheet() != INFORMATION_SHEET {
            problems.push(format!(
                "{at}: a formula cell outside sheet {INFORMATION_SHEET}"
            ));
            continue;
        }
        if let Some(source) = INFO_SOURCES
            .iter()
            .find(|source| source_cell(source) == *address)
        {
            let assumed = CalcValue::error(source.assumed);
            match (&cell.verdict, raw(address)) {
                (Verdict::NotModeled { .. }, Some(value)) if identical(value, &assumed) => {}
                (verdict, value) => problems.push(format!(
                    "{at} {}: the source of rows that assume {}: {}, Excel's raw value {value:?}",
                    source.formula,
                    shown_value(&assumed),
                    verdict.name()
                )),
            }
            continue;
        }
        let row = (address.column() == FORMULA_COLUMN)
            .then(|| rows.get(&address.row()))
            .flatten();
        let Some(row) = row else {
            problems.push(format!(
                "{at}: a formula cell that no oracle row has ({})",
                cell.verdict.name()
            ));
            continue;
        };
        let formula = &row.formula;
        match (row.owner, info.sources_read.get(&row.row), &row.outcome, &cell.verdict) {
            (Some(owner), _, _, verdict) => {
                if owner.bead.split(", ").any(|bead| bead == "dc-nzpw") {
                    problems.push(format!(
                        "{at} {formula}: a row of dc-nzpw names its own bead as the owner of what it refuses ({})",
                        verdict.name()
                    ));
                } else if !matches!(verdict, Verdict::NotModeled { feature, .. } if *feature == owner.feature())
                {
                    problems.push(format!(
                        "{at} {formula}: a row owned by {} is {}, not not_modeled with its owner: {verdict:?}",
                        owner.bead,
                        verdict.name()
                    ));
                }
            }
            (None, Some(read), Ok(value), Verdict::NotModeled { .. }) => match raw(address) {
                Some(excel) if identical(excel, value) => conditional_agreed += 1,
                excel => problems.push(format!(
                    "{at} {formula}: conditional on {read}: calc_func::invoke gave {}, Excel's raw value {excel:?}",
                    shown_value(value)
                )),
            },
            (None, None, Ok(_), Verdict::Matched { .. }) => {}
            (None, None, Ok(_), Verdict::Mismatched { expected, actual, .. }) => problems.push(format!(
                "{at} {formula}: calc_func::invoke gave {}, Excel's raw value is {}",
                shown_value(actual),
                shown_value(expected)
            )),
            (None, _, outcome, verdict) => problems.push(format!(
                "{at} {formula}: {} with outcome {outcome:?}: {verdict:?}",
                verdict.name()
            )),
        }
    }
    // Each row's and source's formula as Excel loaded it, and whether it is a dynamic-array
    // formula.
    for formula in &capture.formulas {
        let address = &formula.cell;
        let at = format!("{}!{}", address.sheet(), address.a1());
        let expected = match INFO_SOURCES
            .iter()
            .find(|source| source_cell(source) == *address)
        {
            Some(source) => Some((source.formula.to_owned(), true)),
            None if address.sheet() == INFORMATION_SHEET && address.column() == FORMULA_COLUMN => {
                rows.get(&address.row()).map(|row| {
                    (
                        information_loaded(&row.formula),
                        info.dynamic.contains(&row.row),
                    )
                })
            }
            None => None,
        };
        let Some((text, dynamic)) = expected else {
            continue;
        };
        if formula.formula.as_deref() != Some(text.as_str()) || formula.dynamic_array != dynamic {
            problems.push(format!(
                "{at} {text}: Excel loaded the formula {:?} (dynamic-array {}), where {text:?} (dynamic-array {dynamic}) is expected",
                formula.formula, formula.dynamic_array
            ));
        }
    }
    // An unrecorded row: no raw value, the capture's value failure naming #BUSY!'s code, and
    // Excel's saved cache of the cell equal to the value calc_func::invoke gave.
    let session = capture.session.as_ref();
    for (row, basis) in info.rows.oracle.iter().zip(&info.basis) {
        if !matches!(basis, InfoBasis::Unrecorded(..)) {
            continue;
        }
        let address = oracle_cell(INFORMATION_SHEET, row.row);
        let at = format!("{}!{}", address.sheet(), address.a1());
        let Ok(value) = &row.outcome else {
            problems.push(format!("{at} {}: an unrecorded row refuses", row.formula));
            continue;
        };
        let named = format!("Excel error code {BUSY_CODE} ");
        let failed = session.is_some_and(|session| {
            session.failures.iter().any(|failure| {
                failure.stage == "value"
                    && failure.cell.as_ref() == Some(&address)
                    && failure.message.starts_with(&named)
            })
        });
        let cache = session
            .and_then(|session| session.saved.iter().find(|saved| saved.cell == address))
            .and_then(|saved| match &saved.observation {
                Observation::ExcelSavedCache(cache) => Some(cache),
                _ => None,
            });
        match (raw(&address), failed, cache) {
            (None, true, Some(cache)) if identical(cache.value(), value) => {}
            (excel, failed, cache) => problems.push(format!(
                "{at} {}: an unrecorded row: Excel's raw value {excel:?}, a value failure naming error code {BUSY_CODE} {}, Excel's saved cache {:?}, where calc_func::invoke gave {}",
                row.formula,
                if failed { "recorded" } else { "absent" },
                cache.map(SavedCache::text),
                shown_value(value)
            )),
        }
    }
    for outside in &report.outside_inventory {
        problems.push(format!(
            "{}!{}: the capture's inventory lacks this cell",
            outside.cell.sheet(),
            outside.cell.a1()
        ));
    }
    Ok((report, problems, conditional_agreed))
}

/// A capture record of the information oracle workbook built here in memory: a version 2 live
/// capture of the workbook as authored by the committed worker, listing every row's formula cell,
/// with the text Excel loads it as and its dynamic-array flag, and every source cell, with the raw
/// value `observe` gives a row and `source_value` a source. For an unrecorded row it holds what
/// the capture adapter records of an error it has no code for: no raw value, a value failure
/// naming #BUSY!'s code, and the saved cache of the value `observe` gives.
fn information_capture(
    info: &InfoRows,
    observe: impl Fn(&OracleRow) -> Option<CalcValue>,
    source_value: impl Fn(&InfoSource) -> CalcValue,
) -> CaptureRecord {
    let source = format!("sha256:{}", sha256_hex(INFORMATION_WORKBOOK));
    let mut formulas = Vec::new();
    let mut observations = Vec::new();
    let mut failures = Vec::new();
    let mut saved = Vec::new();
    for (row, basis) in info.rows.oracle.iter().zip(&info.basis) {
        let cell = oracle_cell(INFORMATION_SHEET, row.row);
        formulas.push(FormulaCell {
            cell: cell.clone(),
            kind: FormulaKind::Ordinary,
            formula: Some(information_loaded(&row.formula)),
            dynamic_array: info.dynamic.contains(&row.row),
        });
        if let InfoBasis::Unrecorded(..) = basis {
            failures.push(CaptureFailure {
                stage: "value".to_owned(),
                cell: Some(cell.clone()),
                message: format!("Excel error code {BUSY_CODE} is not one this value model holds"),
            });
            if let Some(value) = observe(row) {
                saved.push(CellObservation {
                    cell,
                    observation: Observation::ExcelSavedCache(information_saved_cache(&value)),
                });
            }
            continue;
        }
        if let Some(value) = observe(row) {
            observations.push(CellObservation {
                cell,
                observation: Observation::ExcelRaw(value),
            });
        }
    }
    for cell in &INFO_SOURCES {
        formulas.push(FormulaCell {
            cell: source_cell(cell),
            kind: FormulaKind::Ordinary,
            formula: Some(cell.formula.to_owned()),
            dynamic_array: true,
        });
        observations.push(CellObservation {
            cell: source_cell(cell),
            observation: Observation::ExcelRaw(source_value(cell)),
        });
    }
    CaptureRecord {
        case: INFORMATION_CAPTURE_CASE.to_owned(),
        source: source.clone(),
        acquisition: Acquisition::LiveCapture,
        environment: None,
        sheets: vec![SheetEntry {
            name: INFORMATION_SHEET.to_owned(),
            visibility: SheetVisibility::Visible,
        }],
        formulas,
        observations,
        session: Some(CaptureSession {
            revision: AUTHORED_REVISION,
            operations: Vec::new(),
            context: package_context(INFORMATION_WORKBOOK).fingerprint(),
            tool: format!("worker sha256:{}", sha256_hex(CAPTURE_WORKER)),
            input_package: source,
            output_package: None,
            failures,
            shared_ranges: Vec::new(),
            saved,
            reopened: Vec::new(),
        }),
    }
}

/// An observation that agrees with every computed row, its own value; a refused row is observed as
/// a text that differs from anything calc_func gives, so its verdict shows it is never compared.
fn information_agreeing(row: &OracleRow) -> Option<CalcValue> {
    match &row.outcome {
        Ok(value) => Some(value.clone()),
        Err(_) => Some(text_value("an observation never compared")),
    }
}

/// A kind of row a probe of the comparison changes, and how to select one.
type RowProbe<'a> = (&'static str, Box<dyn Fn(&OracleRow) -> bool + 'a>);

#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test)]
fn dc_nzpw__excel_comparison() {
    const CASE: &str = "dc_nzpw__excel_comparison";
    let mut sheet = information_sheet();
    let info = information_rows(&mut sheet, true);
    let rows = &info.rows;
    let all = rows.oracle.len();
    let refused = rows.refused();
    let conditional = info.sources_read.len();
    let unrecorded = info.count(|basis| matches!(basis, InfoBasis::Unrecorded(..)));
    let calculated = all - refused - conditional - unrecorded;
    assert!(
        rows.oracle.iter().zip(&info.basis).all(|(row, basis)| {
            row.owner.is_some()
                == (row.outcome.is_err() || matches!(basis, InfoBasis::Unrecorded(..)))
        }),
        "a refused or unrecorded row names its owner, a calculated row none"
    );
    assert!(
        info.sources_read
            .keys()
            .all(|row| rows.oracle[*row as usize - 1].outcome.is_ok()),
        "every conditional row computes"
    );

    // The comparison itself, on capture records built here in memory, which carry no Excel
    // evidence: a capture that observes each computed row's own value and each source's assumed
    // error matches every calculated row, agrees on every conditional row and counts every refused
    // row not modelled with its owner; a capture that changes one of them is exactly one problem,
    // at its cell.
    let assumed = |source: &InfoSource| CalcValue::error(source.assumed);
    let (report, problems, agreed) = information_comparison(
        &information_capture(&info, information_agreeing, assumed),
        &info,
    )
    .unwrap_or_else(|refusal| panic!("judge refused the in-memory capture: {refusal}"));
    assert_eq!(problems, Vec::<String>::new());
    assert_eq!(agreed, conditional);
    let counts = &report.counts;
    assert_eq!(
        (
            counts.denominator,
            counts.calculated,
            counts.matched,
            counts.not_modeled,
            counts.mismatched + counts.not_run + counts.unproven + counts.stale,
            counts.evaluator_calls
        ),
        (
            (all + INFO_SOURCES.len()) as u64,
            calculated as u64,
            calculated as u64,
            (refused + conditional + unrecorded + INFO_SOURCES.len()) as u64,
            0,
            all as u64
        ),
        "{}",
        report.summary()
    );
    let probes: [RowProbe<'_>; 4] = [
        (
            "a calculated row",
            Box::new(|row: &OracleRow| {
                row.owner.is_none()
                    && !info.sources_read.contains_key(&row.row)
                    && row
                        .outcome
                        .as_ref()
                        .is_ok_and(|value| value.as_number().is_some_and(|n| n != 0.0))
            }),
        ),
        (
            "a calculated logical row",
            Box::new(|row: &OracleRow| {
                row.owner.is_none()
                    && !info.sources_read.contains_key(&row.row)
                    && row
                        .outcome
                        .as_ref()
                        .is_ok_and(|value| value.as_logical().is_some())
            }),
        ),
        (
            "a conditional row",
            Box::new(|row: &OracleRow| info.sources_read.contains_key(&row.row)),
        ),
        (
            "an unrecorded row's saved cache",
            Box::new(|row: &OracleRow| {
                matches!(info.basis[row.row as usize - 1], InfoBasis::Unrecorded(..))
            }),
        ),
    ];
    let mut changed_cells = Vec::new();
    for (what, select) in &probes {
        let changed = rows
            .oracle
            .iter()
            .find(|row| select(row))
            .unwrap_or_else(|| panic!("{what}"))
            .row;
        let capture = information_capture(
            &info,
            |row| {
                let own = information_agreeing(row)?;
                if row.row != changed {
                    return Some(own);
                }
                Some(match own.core() {
                    CoreValue::Number(n) => CalcValue::number(f64::from_bits(n.to_bits() ^ 1)),
                    CoreValue::Logical(b) => CalcValue::logical(!b),
                    _ => text_value("changed"),
                })
            },
            assumed,
        );
        let (_, problems, _) = information_comparison(&capture, &info)
            .unwrap_or_else(|refusal| panic!("judge refused the in-memory capture: {refusal}"));
        let cell = format!("{INFORMATION_SHEET}!{}", encode_a1(changed, FORMULA_COLUMN));
        assert!(
            problems.len() == 1 && problems[0].starts_with(&format!("{cell} ")),
            "{what} changed at {cell} gives {problems:?}"
        );
        changed_cells.push(cell);
    }
    let (_, problems, _) = information_comparison(
        &information_capture(&info, information_agreeing, |source| {
            if source.address == "F2" {
                CalcValue::error(WorksheetErrorCode::Calc)
            } else {
                assumed(source)
            }
        }),
        &info,
    )
    .unwrap_or_else(|refusal| panic!("judge refused the in-memory capture: {refusal}"));
    assert!(
        problems.len() == 1 && problems[0].starts_with(&format!("{INFORMATION_SHEET}!F2 ")),
        "a source observed with another error than assumed gives {problems:?}"
    );
    // A loaded formula text without Excel's @, a dynamic-array row loaded as a legacy formula, and
    // an unrecorded row without its value failure are each exactly one problem, at its cell.
    let at_row = rows
        .oracle
        .iter()
        .find(|row| information_loaded(&row.formula).contains('@'))
        .expect("a row Excel loads with @");
    let dynamic_row = *info.dynamic.first().expect("a dynamic-array row");
    let unrecorded_row = rows
        .oracle
        .iter()
        .zip(&info.basis)
        .find(|(_, basis)| matches!(basis, InfoBasis::Unrecorded(..)))
        .map(|(row, _)| row.row)
        .expect("an unrecorded row");
    let mut altered_cells = Vec::new();
    for (what, row) in [
        ("the written text as loaded", at_row.row),
        ("a legacy formula loaded", dynamic_row),
        ("no value failure", unrecorded_row),
    ] {
        let cell = oracle_cell(INFORMATION_SHEET, row);
        let mut capture = information_capture(&info, information_agreeing, assumed);
        match what {
            "the written text as loaded" => {
                capture
                    .formulas
                    .iter_mut()
                    .find(|formula| formula.cell == cell)
                    .expect("the row's formula cell")
                    .formula = Some(at_row.formula.clone());
            }
            "a legacy formula loaded" => {
                capture
                    .formulas
                    .iter_mut()
                    .find(|formula| formula.cell == cell)
                    .expect("the row's formula cell")
                    .dynamic_array = false;
            }
            _ => capture
                .session
                .as_mut()
                .expect("a version 2 record")
                .failures
                .retain(|failure| failure.cell.as_ref() != Some(&cell)),
        }
        let (_, problems, _) = information_comparison(&capture, &info)
            .unwrap_or_else(|refusal| panic!("judge refused the in-memory capture: {refusal}"));
        let at = format!("{INFORMATION_SHEET}!{}", encode_a1(row, FORMULA_COLUMN));
        assert!(
            problems.len() == 1 && problems[0].starts_with(&format!("{at} ")),
            "{what} at {at} gives {problems:?}"
        );
        altered_cells.push(format!("{what} at {at}"));
    }
    let mut self_owned = information_rows(&mut information_sheet(), true);
    let row = self_owned
        .rows
        .oracle
        .iter_mut()
        .find(|row| row.outcome.is_err())
        .expect("a refused row");
    row.owner = Some(Owner {
        bead: "dc-nzpw",
        behaviour: "a refusal of the bead under test",
    });
    let self_cell = format!("{INFORMATION_SHEET}!{}", encode_a1(row.row, FORMULA_COLUMN));
    let (_, problems, _) = information_comparison(
        &information_capture(&self_owned, information_agreeing, assumed),
        &self_owned,
    )
    .unwrap_or_else(|refusal| panic!("judge refused the in-memory capture: {refusal}"));
    assert!(
        problems.len() == 1
            && problems[0].starts_with(&format!("{self_cell} "))
            && problems[0].contains("names its own bead"),
        "a row owned by dc-nzpw at {self_cell} gives {problems:?}"
    );
    note!(
        "ACCEPT-NOTE: {CASE}: on in-memory captures of {INFORMATION_WORKBOOK_PATH} (no Excel evidence; {all} rows: {calculated} calculated, {conditional} conditional on a source, {refused} refused, {unrecorded} unrecorded; {} sources): calc_verdict::judge's report through the comparison matches every calculated row of the agreeing capture, agrees on every conditional row, counts every refused and unrecorded row and source not modeled with its owner, and finds every loaded formula text and dynamic-array flag as expected; a change at {} is exactly one problem each, at that cell; F2 observed as #CALC! instead of the assumed #SPILL! is exactly one problem, at F2; {} is exactly one problem each, at that cell; the refused row at {self_cell} given dc-nzpw as its owner is exactly one problem, at that cell",
        INFO_SOURCES.len(),
        changed_cells.join(", "),
        altered_cells.join(", ")
    );

    // Excel's capture of the workbook.
    if let Some(missing) = MissingCapture::recognize(INFORMATION_CAPTURE) {
        assert_eq!(
            (
                missing.case.as_str(),
                missing.fixture.as_str(),
                missing.command.as_str()
            ),
            (
                INFORMATION_CAPTURE_CASE,
                INFORMATION_WORKBOOK_PATH,
                INFORMATION_CAPTURE_COMMAND
            ),
            "the absent-capture marker names this capture"
        );
        note!("ACCEPT-NOT-RUN: {CASE}: {missing}");
        return;
    }
    let capture =
        CaptureRecord::decode(INFORMATION_CAPTURE).expect("the committed capture decodes");
    assert_eq!(capture.case, INFORMATION_CAPTURE_CASE);
    assert_eq!(capture.acquisition, Acquisition::LiveCapture);
    let session = capture
        .session
        .as_ref()
        .expect("the committed capture is a version 2 record");
    let workbook = format!("sha256:{}", sha256_hex(INFORMATION_WORKBOOK));
    if capture.source != workbook {
        note!(
            "ACCEPT-NOT-RUN: {CASE}: awaiting capture: the committed capture is of {}, and the oracle workbook as committed is {workbook} (cargo xtask capture --pending takes it)",
            capture.source
        );
        return;
    }
    let isolation: Vec<_> = session.failures_at(STAGE_ISOLATION).collect();
    assert!(
        isolation.is_empty(),
        "the committed capture records isolation failures: {isolation:?}"
    );
    let environment = capture
        .environment
        .as_ref()
        .expect("the capture records Excel's environment");
    let (report, problems, agreed) = information_comparison(&capture, &info)
        .unwrap_or_else(|refusal| panic!("calc_verdict::judge refuses the capture: {refusal}"));
    assert!(
        problems.is_empty(),
        "{} problems on sheet {INFORMATION_SHEET} of the verdict on Excel {}'s capture ({}):\n{}",
        problems.len(),
        environment.target.excel_build(),
        report.summary(),
        problems.join("\n")
    );
    assert_eq!(agreed, conditional);
    assert_eq!(
        (report.counts.calculated, report.counts.matched),
        (calculated as u64, calculated as u64),
        "{}",
        report.summary()
    );
    note!(
        "ACCEPT-NOTE: {CASE}: Excel comparison through calc_verdict::judge against Excel {}'s capture of {INFORMATION_WORKBOOK_PATH}: {} (every calculated row matched; every row that reads a source not modeled, with Excel's raw value equal to calc_func::invoke's ({agreed} of {conditional}); every source's raw value the assumed error; every refused row not modeled with its owner; every unrecorded row ({unrecorded}) not modeled with its owner, with no raw value, the capture's value failure naming error code {BUSY_CODE} and Excel's saved cache equal to calc_func::invoke's value; every row's and source's formula as Excel loaded it, {} of them with @, and rows {:?} dynamic-array formulas); the whole workbook: {}",
        environment.target.excel_build(),
        sheet_verdicts(&report, rows),
        rows.oracle
            .iter()
            .filter(|row| information_loaded(&row.formula).contains('@'))
            .count(),
        info.dynamic,
        report.summary()
    );
}

// ---------------------------------------------------------------------------------------------
// dc-xrxb: the file prefix of each catalogued named function, against the package Excel saved.

#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test)]
fn dc_xrxb__prefix_metadata_matches_excel() {
    dc_xrxb_support::prefix_metadata_matches_excel();
}

#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test)]
fn dc_j9k__questions_preserve_catalogue_captures() {
    dc_xrxb_support::catalogue_context_captures_stay_valid();
}
