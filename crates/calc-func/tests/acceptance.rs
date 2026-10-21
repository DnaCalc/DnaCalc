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
    ArrayShape, CalcArray, CalcValue, CallableArityShape, CallableValue, CellKey, CoreValue,
    ExcelText, GridArea, GridError, IdAllocator, IdError, MAX_COLUMNS, MAX_ROWS, NameId, NodeId,
    NumericMode, NumericTargetId, OpaqueCallable, ReferenceError, ReferenceLike, SheetId, TableId,
    WorkbookId, WorksheetErrorCode,
};
use calc_func::{
    ArgumentDemand, CellReader, EvalServices, FunctionCallScratch, FunctionCallTarget,
    FunctionFault, FunctionId, Notation, ParameterClass, PreparedArguments, RegistrationError,
    Unreadable, catalogue, invoke,
};
use calc_verdict::{
    Acquisition, CalculationContext, CaptureFailure, CaptureRecord, CaptureSession, CellAddress,
    CellObservation, CellResult, Evaluation, EvaluationRecord, FormulaCell, FormulaKind,
    JudgeError, MissingCapture, Observation, Provenance, ResultSet, RunIdentity, STAGE_INVENTORY,
    STAGE_ISOLATION, SheetEntry, SheetVisibility, Verdict, VerdictPolicy, VerdictReport, judge,
    sha256_hex,
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
    CoreValue,
    CalcArray,
    ArrayShape,
    ExcelText,
    WorksheetErrorCode,
    ReferenceLike,
    ReferenceError,
    CellKey,
    GridArea,
    GridError,
    WorkbookId,
    SheetId,
    NameId,
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
const CALC_VALUE_SOURCES: [(&str, &str); 5] = [
    ("lib", include_str!("../../calc-value/src/lib.rs")),
    ("grid", include_str!("../../calc-value/src/grid.rs")),
    ("id", include_str!("../../calc-value/src/id.rs")),
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
    let mut arity_ids = BTreeSet::new();
    let mut computed = Vec::new();
    let mut refused = Vec::new();
    let mut lazy = Vec::new();
    for target in catalogue.targets() {
        let arity = target.arity();
        match target.notation() {
            Notation::Infix => assert_eq!((arity.min, arity.max), (2, 2), "{target:?}"),
            Notation::Prefix | Notation::Postfix => {
                assert_eq!((arity.min, arity.max), (1, 1), "{target:?}")
            }
            Notation::Call => assert!(arity.min <= arity.max),
        }
        assert!(
            arity.min >= 1,
            "{target:?}: every admitted entry takes an argument"
        );
        assert!(
            target.parameter_class(arity.max - 1).is_some(),
            "{target:?}"
        );
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
        let mut too_few = Recorded::numbers(arity.min - 1);
        assert_eq!(
            run(target, &mut too_few, &fast),
            Err(FunctionFault::ArityMismatch {
                id: target.id(),
                supplied: arity.min - 1
            })
        );
        assert!(too_many.asked.is_empty() && too_few.asked.is_empty());
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
    assert_eq!(lazy, [iff.id()], "only IF is lazy");
    note!(
        "ACCEPT-NOTE: {CASE}: {} catalogue IDs; the dispatch, name, help, arity and oracle views each enumerate the same {}; at the minimum arity {} entries compute ({:?}) and {} refuse under their own ID ({:?}); lazy entries {:?}",
        dispatch_ids.len(),
        dispatch_ids.len(),
        computed.len(),
        computed.iter().map(|(id, _)| *id).collect::<Vec<_>>(),
        refused.len(),
        refused,
        lazy
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
// code's own consistency check and says nothing about Excel: no hand-written expectation has been
// compared with Excel. A row labelled `unmeasured choice` encodes a choice of this implementation
// that no Excel result has settled (which operand's error wins, the order SUM reads a range in,
// an overflow against a later error, a zero's sign, an underflow of number text).
//
// Each row names the bead that owns the Excel behaviour it shows. A row of cfunc-m1.2 computes a
// value, and the Excel comparison must find it equal to Excel's. A row whose behaviour another
// bead owns (text collation and nearly equal numbers, the near-zero adjustment of a formula's
// final + or -, ^ beyond the powers computed here, number text read by locale rules, a number's
// General text, array lifting, subnormal results) is run and asserted the same way, whether
// calc-func computes it or refuses it, and is published to the comparison as not modeled, with
// its owner: it is counted as not modeled and never as matched.
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
// computes from them and the capture. Until the capture is taken that path holds the
// absent-capture marker, and each case prints ACCEPT-NOT-RUN with calc-verdict's MissingCapture,
// so the runner records it as not run.
//
// The retained witness `migration/witnesses/w111-extended-power.json` holds Excel's results of the
// POWER function. The mixed-operand case applies it to the ^ operator, which assumes that ^
// computes POWER; the oracle workbook's ^ rows test that assumption once they are captured.

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
const GENERATED_WORKBOOKS: [&str; 4] = [
    "cfunc_m1_2/oracle.xlsx",
    "cfunc_m3_2/oracle.xlsx",
    "cfunc_m3_4/oracle.xlsx",
    "cfunc_m3_5/oracle.xlsx",
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
const ERROR_LITERALS: [(WorksheetErrorCode, &str); 7] = [
    (WorksheetErrorCode::Null, "#NULL!"),
    (WorksheetErrorCode::Div0, "#DIV/0!"),
    (WorksheetErrorCode::Value, "#VALUE!"),
    (WorksheetErrorCode::Ref, "#REF!"),
    (WorksheetErrorCode::Name, "#NAME?"),
    (WorksheetErrorCode::Num, "#NUM!"),
    (WorksheetErrorCode::NA, "#N/A"),
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
    a == b && a.as_number().map(f64::to_bits) == b.as_number().map(f64::to_bits)
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

/// Text ordering and case-insensitive text equality.
const COLLATION: Owner = Owner {
    bead: "cdoc-jpha",
    behaviour: "Excel's text collation",
};
const NEAR_EQUAL: Owner = Owner {
    bead: "cdoc-jpha",
    behaviour: "Excel's comparison of nearly equal numbers",
};
const NEAR_ZERO: Owner = Owner {
    bead: "dc-r4ci",
    behaviour: "the near-zero adjustment of + and - by the formula's final operation",
};
const POWER_ALGORITHM: Owner = Owner {
    bead: "cfunc-m3.3, cfunc-m3.23",
    behaviour: "^ through a pinned power implementation",
};
const NUMBER_TEXT: Owner = Owner {
    bead: "cfml-m2.6",
    behaviour: "text read as a number or a logical by Excel's number-text rules",
};
const GENERAL_TEXT: Owner = Owner {
    bead: "cfml-m2.6",
    behaviour: "a number's text in the General format",
};
const ARRAY_LIFT: Owner = Owner {
    bead: "cfml-m3.4",
    behaviour: "an operator or IF lifted over an array",
};
const SUBNORMAL: Owner = Owner {
    bead: "cfunc-m3.23",
    behaviour: "subnormal results",
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
}

impl Rows {
    fn new(case: &'static str, sheet: &'static str, quiet: bool) -> Self {
        Rows {
            case,
            sheet,
            oracle: Vec::new(),
            calls: Vec::new(),
            quiet,
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
                let label = match choice {
                    Some(choice) => format!("unmeasured choice: {choice}"),
                    None => "unmeasured".to_owned(),
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

/// Runs a row of cfunc-m1.2: `row!(surface, [args], expect)`, or with a fourth argument naming
/// the unmeasured choice the row encodes. Evaluates to the arguments the call interface asked for.
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

/// A cell of an oracle sheet: a formula (its text, without `=`) or a constant.
#[derive(Debug)]
enum FixtureCell {
    Formula(String),
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
        let cell = match (inner("f"), attribute(tag, "t")) {
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
        };
        assert!(
            cells.insert(decode_a1(reference), cell).is_none(),
            "{reference} repeats"
        );
    }
    cells
}

/// A formula as SpreadsheetML stores it: a function added to Excel after the file format's first
/// version is written with the `_xlfn.` prefix, which Excel requires to recognize it in a file
/// (MAXIFS and MINIFS among the functions these fixtures call).
fn stored_formula(formula: &str) -> String {
    formula
        .replace("MAXIFS(", "_xlfn.MAXIFS(")
        .replace("MINIFS(", "_xlfn.MINIFS(")
}

/// Asserts that the oracle workbook's sheet for this case holds exactly the test sheet's cells
/// and the case's oracle formulas, each in column D at its row, and nothing else.
fn assert_oracle_sheet(rows: &Rows, sheet: &TestSheet) {
    assert_workbook_sheet(ORACLE_WORKBOOK, ORACLE_WORKBOOK_PATH, rows, sheet);
}

/// [`assert_oracle_sheet`] for the committed workbook `package`, at `path`.
fn assert_workbook_sheet(package: &[u8], path: &str, rows: &Rows, sheet: &TestSheet) {
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
    let mut constants = BTreeMap::new();
    for (place, cell) in cells {
        match cell {
            FixtureCell::Formula(formula) => {
                formulas.insert(place, formula);
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
    for (place, value) in &sheet.cells {
        match constants.get(place) {
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
    if !rows.quiet {
        note!(
            "ACCEPT-NOTE: {}: {path} (sha256 {}): sheet {} holds exactly the {} oracle rows' formulas in column D, in row order, and the {} cells the rows read",
            rows.case,
            sha256_hex(package),
            rows.sheet,
            expected.len(),
            sheet.cells.len()
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
    let rows: BTreeMap<u32, &OracleRow> = case.oracle.iter().map(|row| (row.row, row)).collect();
    let mut problems = Vec::new();
    for cell in &report.cells {
        let address = &cell.formula.cell;
        if address.sheet() != case.sheet {
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
        "ACCEPT-NOTE: {}: {} rows through calc_func::invoke, each asserted against a hand-written expectation as the code's own consistency check and each with its formula on sheet {} of the oracle workbook: {} of cfunc-m1.2 ({} of them unmeasured choices) and {} whose behaviour another bead owns ({} of them refused by calc-func)",
        rows.case,
        rows.oracle.len(),
        rows.sheet,
        rows.oracle.len() - elsewhere,
        rows.choices(),
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
    assert_eq!(
        capture.source, workbook,
        "the capture's source is not the oracle workbook as committed"
    );
    assert_eq!(
        session.input_package, workbook,
        "the capture's input package is not the oracle workbook as committed"
    );
    let worker = format!("worker sha256:{}", sha256_hex(CAPTURE_WORKER));
    assert!(
        session.tool.split("; ").any(|part| part == worker),
        "the capture-tool revision {:?} does not name the committed worker ({worker})",
        session.tool
    );
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
/// of 14 significant digits that differ in the last.
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
    // Decimal text is read as its nearest binary64.
    row!(r, s, PLUS, [txt("0.1"), num("0")], N(0.1));
    row!(
        r,
        s,
        PLUS,
        [txt("1e-400"), num("0")],
        N(0.0),
        "number text below the smallest subnormal reads as +0"
    );
    // Text forms that Excel's locale rules may read as numbers, and a 16th or 17th significant
    // digit, are refused, not guessed.
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
    owned_by!(
        r,
        s,
        NUMBER_TEXT,
        PLUS,
        [txt("1234567890123456"), num("0")],
        Refused(Some(0))
    );
    owned_by!(
        r,
        s,
        NUMBER_TEXT,
        PLUS,
        [txt("1.2345678901234567"), num("0")],
        Refused(Some(0))
    );
    owned_by!(
        r,
        s,
        NUMBER_TEXT,
        PLUS,
        [num("0"), txt("1e400")],
        Refused(Some(1))
    );
    // Results: an overflow is #NUM!, division by 0 is #DIV/0!, a zero is +0 whatever its sign,
    // and a subnormal result is refused.
    row!(
        r,
        s,
        TIMES,
        [num("1E308"), num("10")],
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
    owned_by!(
        r,
        s,
        SUBNORMAL,
        DIVIDE,
        [num("2.2250738585072014E-308"), num("2")],
        Refused(None)
    );
    // ^ computes only the powers that need no power algorithm; the others are refused.
    row!(r, s, POWER, [txt("2"), num("2")], N(4.0));
    row!(r, s, POWER, [logical(true), num("2")], N(1.0));
    owned_by!(
        r,
        s,
        POWER_ALGORITHM,
        POWER,
        [num("2"), num("3")],
        Refused(None)
    );
    owned_by!(
        r,
        s,
        POWER_ALGORITHM,
        POWER,
        [num("10"), num("-2")],
        Refused(None)
    );
    owned_by!(
        r,
        s,
        POWER_ALGORITHM,
        POWER,
        [num("2"), num("0.5")],
        Refused(None)
    );
    row!(r, s, POWER, [num("3"), num("1")], N(3.0));
    row!(r, s, NEGATE, [num("0")], N(0.0), ZERO);
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
    // compares as the other operand's empty value, and text compares ignoring case. Text
    // ordering, case-insensitive equality and nearly equal numbers follow Excel's collation and
    // comparison rules, which another bead owns.
    row!(r, s, EQ, [txt("1"), num("1")], L(false));
    row!(r, s, NE, [cell(s, "A1"), num("2")], L(true));
    row!(r, s, LT, [num("1"), txt("a")], L(true));
    row!(r, s, LT, [txt("a"), logical(true)], L(true));
    row!(r, s, GT, [logical(false), num("1E308")], L(true));
    owned_by!(r, s, COLLATION, EQ, [txt("a"), txt("A")], L(true));
    row!(r, s, EQ, [cell(s, "A5"), num("0")], L(true));
    row!(r, s, EQ, [cell(s, "A5"), txt("")], L(true));
    row!(r, s, EQ, [cell(s, "A5"), logical(false)], L(true));
    row!(r, s, EQ, [cell(s, "A5"), cell(s, "A6")], L(true));
    row!(r, s, LT, [cell(s, "A5"), num("1")], L(true));
    owned_by!(r, s, COLLATION, LT, [txt("abc"), txt("abd")], L(true));
    owned_by!(r, s, COLLATION, GT, [txt("B"), txt("a")], L(true));
    owned_by!(r, s, COLLATION, LT, [txt("a1"), txt("aa")], L(true));
    owned_by!(r, s, COLLATION, LT, [txt("10"), txt("9")], L(true));
    owned_by!(r, s, COLLATION, LT, [txt(""), txt("a")], L(true));
    owned_by!(r, s, COLLATION, LE, [txt("a"), txt("A")], L(true));
    row!(r, s, GE, [num("1"), num("1")], L(true));
    row!(r, s, LE, [num("1"), num("0")], L(false));
    row!(r, s, GT, [logical(true), logical(false)], L(true));
    row!(r, s, EQ, [txt("\u{e9}"), txt("\u{e9}")], L(true));
    row!(r, s, EQ, [num("1"), num("2")], L(false));
    // Nearly equal numbers and text that needs Excel's collation are refused.
    owned_by!(
        r,
        s,
        NEAR_EQUAL,
        EQ,
        [arg("0.1+0.2", CalcValue::number(0.1 + 0.2)), num("0.3")],
        Refused(None)
    );
    owned_by!(
        r,
        s,
        COLLATION,
        EQ,
        [txt("\u{e9}"), txt("\u{c9}")],
        Refused(None)
    );
    owned_by!(r, s, COLLATION, LT, [txt("a-b"), txt("ab")], Refused(None));
    owned_by!(
        r,
        s,
        COLLATION,
        LT,
        [txt("apple pie"), txt("banana")],
        Refused(None)
    );
    owned_by!(r, s, COLLATION, LT, [txt("a"), txt("B")], L(true));
    owned_by!(r, s, COLLATION, EQ, [txt("A"), txt("a")], L(true));
    owned_by!(
        r,
        s,
        NEAR_EQUAL,
        EQ,
        [arg("1+1E-15", CalcValue::number(1.0 + 1e-15)), num("1")],
        Refused(None)
    );

    // Concatenation: logicals as TRUE and FALSE, a blank as empty text; a number's text needs
    // the General format, which is refused.
    row!(r, s, AMPERSAND, [txt("a"), txt("b")], t("ab"));
    row!(r, s, AMPERSAND, [txt("a"), logical(true)], t("aTRUE"));
    row!(r, s, AMPERSAND, [txt("a"), logical(false)], t("aFALSE"));
    row!(r, s, AMPERSAND, [cell(s, "A5"), txt("x")], t("x"));
    row!(r, s, AMPERSAND, [cell(s, "A2"), cell(s, "A4")], t("TRUEa"));
    owned_by!(
        r,
        s,
        GENERAL_TEXT,
        AMPERSAND,
        [txt("a"), num("1")],
        Refused(Some(1))
    );
    owned_by!(
        r,
        s,
        GENERAL_TEXT,
        AMPERSAND,
        [num("1"), txt("a")],
        Refused(Some(0))
    );
    // A joined text of 32767 characters is a cell's longest; one more is #VALUE!.
    let shorter = "a".repeat(32_766);
    row!(
        r,
        s,
        AMPERSAND,
        [cell(s, "C1"), txt("b")],
        t(&format!("{shorter}b"))
    );
    row!(
        r,
        s,
        AMPERSAND,
        [cell(s, "C2"), txt("b")],
        E(WorksheetErrorCode::Value)
    );

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
        [num("1E308"), num("1E308")],
        E(WorksheetErrorCode::Num)
    );
    row!(
        r,
        s,
        SUM,
        [num("1E308"), num("1E308"), num("-1E308")],
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
    owned_by!(
        r,
        s,
        NUMBER_TEXT,
        IF,
        [txt("1"), num("1"), num("2")],
        Refused(Some(0))
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
    let conditions = CalcArray::from_rows(vec![vec![
        CalcValue::logical(true),
        CalcValue::logical(false),
    ]])
    .expect("a 1-by-2 array");
    owned_by!(
        r,
        s,
        ARRAY_LIFT,
        IF,
        [
            arg("{TRUE,FALSE}", CalcValue::array(conditions)),
            num("1"),
            num("2")
        ],
        Refused(Some(0))
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
    let nested = CalcValue::array(CalcArray::from_scalar(CalcValue::number(1.0)));
    for element in [nested, a1_reference()] {
        let held = CalcArray::from_rows(vec![vec![CalcValue::number(1.0), element.clone()]])
            .expect("a 1-by-2 array");
        let mut given = Recorded::new(vec![CalcValue::number(1.0), CalcValue::array(held)]);
        assert_eq!(
            run(sum, &mut given, &fast),
            Err(FunctionFault::NotImplemented {
                id: sum.id(),
                argument: Some(1)
            }),
            "SUM(1, an array holding 1 and {element:?})"
        );
    }
    note!(
        "ACCEPT-NOTE: {CASE}: IF with a NaN, +inf, -inf or 5e-324 condition is refused naming the condition and asks for no branch; SUM(1, an array holding 1 and a nested array) and SUM(1, an array holding 1 and a reference) are refused naming the array"
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
    // assumption that ^ computes POWER, which the oracle workbook's ^ rows test once captured:
    // every result ^ computes equals the witness's bit for bit or error for error, and every other
    // row is refused as not modelled.
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
        (56, 19, 55),
        "witness rows computed, of them errors, and refused"
    );
    note!(
        "ACCEPT-NOTE: {CASE}: retained witness of the POWER function (migration/witnesses/w111-extended-power.json, {} rows), applied to ^ on the assumption that ^ computes POWER until the oracle's ^ rows are captured: ^ computed {computed} rows equal to Excel's POWER results ({errors} of them #NUM! or #DIV/0!) and refused {refused} rows that need a power algorithm",
        witness.len()
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
    const ARGUMENT_ORDER: &str = "the first error in argument order";
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
    // A number's text is not modelled, but no number reads as an error, so & still returns the
    // other operand's error.
    row!(r, s, AMPERSAND, [num("1"), err(NA)], E(NA));
    row!(r, s, NEGATE, [err(NA)], E(NA));
    row!(r, s, PERCENT, [err(Div0)], E(Div0));
    row!(r, s, UNARY_PLUS, [err(Ref)], E(Ref));

    // SUM: the first error in argument order; within a reference, area by area in the
    // reference's order and row-major within an area. The cell reader is visited no further.
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
    row!(
        r,
        s,
        SUM,
        [reference(s, &["A1:B2"]), err(Null)],
        E(NA),
        ARGUMENT_ORDER
    );
    s.reads.clear();
    row!(
        r,
        s,
        SUM,
        [err(Ref), reference(s, &["A1:B2"])],
        E(Ref),
        ARGUMENT_ORDER
    );
    assert!(s.reads.is_empty(), "SUM read a reference after an error");
    row!(
        r,
        s,
        SUM,
        [num("1E308"), num("1E308"), err(NA)],
        E(NA),
        "a later error over an earlier overflow of the running total"
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
    /// Refused, for this reason: as not modelled by the kernel, or at admission.
    Unsupported(&'static str),
}

use Basis::{Choice, Documented, Unsupported};

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
        Documented(_) | Choice(_) => assert!(
            result.is_ok(),
            "{formula}: a row of {bead} computes a value"
        ),
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
            Unsupported(reason) => match &result {
                Err(FunctionFault::NotImplemented { .. }) => {
                    format!("refused as not modelled: {reason}")
                }
                Err(fault) => format!("refused with {fault:?}: {reason}"),
                Ok(value) => unreachable!("an unsupported row computed {value:?}"),
            },
        };
        note!(
            "ACCEPT-NOTE: {}: oracle {at} {formula}{cells} -> {} ({label}; not compared with Excel)",
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
            Documented(_) | Unsupported(_) => None,
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
            "the sequential left fold in reading order: 1E16+1 rounds to 1E16 before -1E16 is added (the reduction order cfunc-m3.23 is to document)"
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
        N(0.0),
        Choice(OMITTED_ZERO)
    );
    agg!(
        r,
        s,
        "PRODUCT",
        [num("1E-200"), num("1E-200")],
        Refused(None),
        Unsupported("a product of two non-zero factors that underflows")
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
        Refused(None),
        Unsupported("a square of a non-zero number that underflows")
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
        Refused(Some(0)),
        Unsupported("a reference of several areas")
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
        Refused(None),
        Unsupported("a product of two non-zero factors that underflows")
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
        Unsupported("typed text that Excel's number-text rules may read as a date (cfml-m2.6)")
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
        Refused(Some(0)),
        Unsupported("a reference of several areas")
    );
    agg!(
        r,
        s,
        "COUNTBLANK",
        [err(Ref)],
        Expect::Required(0),
        Unsupported(
            "an error value given for the reference-only parameter is refused at admission; Excel likely returns the error (unmeasured)"
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
        [num("1E308"), num("1E308")],
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
        N(2.0 / 3.0),
        Choice("an array's elements follow the rule for a reference's cells: text 0, TRUE 1")
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
        N(0.0),
        Choice("an array's text is 0, as the rule for a reference's cells reads it")
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
            assert_eq!(target.parameter_class(0), Some(ParameterClass::Reference));
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
        "ACCEPT-NOTE: {CASE}: the catalogue's family 4 holds exactly the {} entries {:?}, each eager; COUNTBLANK takes one reference-only argument and the others 1 to 255 values or references; the per-ID manifest of migration/ports/cfunc-m3.4.json lists the same IDs and spellings",
        family.len(),
        AGGREGATES.map(|(spelling, _)| spelling)
    );

    // COUNTBLANK's reference-only parameter refuses any other value at admission, before its
    // kernel runs.
    let countblank = surface(Notation::Call, "COUNTBLANK");
    for refused in [
        CalcValue::number(1.0),
        CalcValue::array(CalcArray::from_scalar(CalcValue::empty())),
        CalcValue::missing(),
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

    // Values that no formula writes, each refused naming its argument: an array held inside an
    // array argument, and a NaN or an infinity given to a count.
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
        assert_eq!(
            run(target, &mut Recorded::new(vec![nested.clone()]), &fast),
            Err(FunctionFault::NotImplemented {
                id: target.id(),
                argument: Some(0)
            }),
            "{spelling} of an array holding an array"
        );
    }
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
        "ACCEPT-NOTE: {CASE}: through the call interface alone: COUNTBLANK refuses a number, an array and an omitted argument with ReferenceRequired; a direct Empty reads as an omitted argument (unmeasured choice); each aggregate refuses an array held in an array naming it, COUNT and COUNTA refuse NaN and infinity naming them; {unreadable} calls over a reference without a reader or with a refusing one ended in ReferenceUnreadable naming the argument"
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
const ROUNDING: [(&str, u32); 14] = [
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
const MROUND_LIMITATION: &str = "the MROUND page's Known Limitations example: MROUND(6.05,0.1) returns 6.0 while MROUND(7.05,0.1) returns 7.1";
const NO_NEGATIVE_ZERO: &str = "the result is a zero, published as +0: a worksheet number carries no negative zero (the number rule of cfunc-m1.2)";
const NONNUMERIC: &str = "a nonnumeric argument is #VALUE!";
const FIRST_ERROR: &str = "the first argument's error is the result, before any domain rule, as the operators of cfunc-m1.2 read their operands";
const EXAMPLE: &str = "an example row of the function's page";
const SIGN_OF_DIVISOR: &str = "MOD(n, d) = n - d*INT(n/d): the result has the divisor's sign";
const LIFTED: &str = "an array argument, over which Excel lifts the function, is not modelled (array lifting is cfml-m3.4's)";

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
        N(2.67),
        Choice(
            "a number whose shortest decimal has 17 digits is read as its binary64, 2.67499999999999937..., below the half; the 15-digit rule reads 2.67500000000000 and gives 2.68"
        )
    );
    rnd!(
        r,
        s,
        "ROUND",
        [cell(s, "A4"), num("2")],
        N(1.0),
        Choice("read as its binary64 1.00499999999999967...; the 15-digit rule gives 1.01")
    );
    rnd!(
        r,
        s,
        "ROUNDUP",
        [cell(s, "A2"), num("1")],
        N(0.4),
        Choice(
            "read as its binary64 0.30000000000000004440..., above 0.3, so rounding up to one place gives 0.4; the 15-digit rule reads 0.3 and gives 0.3"
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
        N(2.0),
        Choice(
            "read as its binary64 2.99999999999999955...: the integer part is 2, as INT gives (the TRUNC page: TRUNC and INT differ only for negative numbers); the 15-digit rule gives 3"
        )
    );
    rnd!(
        r,
        s,
        "INT",
        [cell(s, "A5")],
        N(2.0),
        Choice("the binary64 2.99999999999999955... is rounded down; the 15-digit rule gives 3")
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
        N(1.4),
        Choice(
            "the exact product: the page's example gives 1.4; the binary64 quotient 6.5 rounds away from zero to 7 multiples, and 7 times 0.2 as typed is 1.4 exactly, converted once to the binary64 nearest 1.4; a binary64 product of 7 and 0.2 gives 1.4000000000000001, which the page's 1.4, shown to 15 digits, does not tell apart"
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
        N(7.1),
        Choice(
            "the exact product: the page's Known Limitations example gives 7.1; the binary64 quotient 70.5 rounds away from zero to 71 multiples, and 71 times 0.1 as typed is 7.1 exactly, converted once to the binary64 nearest 7.1; a binary64 product of 71 and 0.1 gives 7.1000000000000005, which the page's 7.1 does not tell apart"
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
        N(0.1),
        Choice(
            "the binary64 order: the quotient 0.15/0.05 rounds to 2.9999999999999996, whose integer below is 2, and 0.05*2 is 0.1; read as typed decimals 0.15 is 3 multiples of 0.05, so 0.15"
        )
    );
    rnd!(
        r,
        s,
        "FLOOR",
        [num("0.3"), num("0.1")],
        N(0.2),
        Choice(
            "the binary64 order: the quotient 0.3/0.1 rounds to 2.9999999999999996, whose integer below is 2, and 0.1*2 is 0.2; read as typed decimals 0.3 is 3 multiples of 0.1, so 0.3"
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
        N(0.0),
        Choice(
            "the exact decimal rule, kept for MOD although the MROUND page's Known Limitations example rules out counting MROUND's multiples on the decimal readings: the remainder of 0.3 and 0.1 as the typed numbers read is 0; n - d*INT(n/d) in binary64 steps gives 0.09999999999999998"
        )
    );
    rnd!(
        r,
        s,
        "MOD",
        [num("1"), num("0.1")],
        N(0.0),
        Choice(
            "the exact decimal rule, kept for MOD although the MROUND page's Known Limitations example rules out counting MROUND's multiples on the decimal readings: the remainder of 1 and 0.1 as typed is 0; the exact remainder of their binary64 values is 0.09999999999999995"
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
        N(3.0),
        Choice(
            "the exact decimal rule, kept for QUOTIENT although the MROUND page's Known Limitations example rules out counting MROUND's multiples on the decimal readings: 0.3 is 3 times 0.1 as the typed numbers read; the binary64 quotient 2.9999999999999996 gives 2"
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
        Refused(None),
        Unsupported(
            "the result 2E-308 is subnormal, and Excel's handling of a subnormal result is not modelled (subnormal handling is cfunc-m3.23's)"
        )
    );
    rnd!(
        r,
        s,
        "MOD",
        [cell(s, "A6"), num("5.70820472448858E-308")],
        Refused(None),
        Unsupported(
            "the exact remainder of A6's binary64 by 5.70820472448858E-308 is not zero but lies below half the smallest subnormal, so it would convert to 0; it is refused as a subnormal result is, since Excel's handling of a result below the normal range is not modelled (subnormal handling is cfunc-m3.23's)"
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
        N(-4_503_599_627_370_495.0),
        Choice(
            "A5 is read as its binary64 -4503599627370495.5, whose fractional part is removed; the 15-digit reading gives -4503599627370500"
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
        N(9_223_372_036_854_775_808.0),
        Choice(
            "A9 is read as its binary64 2^63, an integer; the 15-digit reading gives 9223372036854779904"
        )
    );
    rnd!(
        r,
        s,
        "TRUNC",
        [cell(s, "A8")],
        N(-max),
        Choice(
            "A8 is read as minus the largest binary64, an integer; the 15-digit reading lies beyond binary64's range and gives #NUM!"
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
        N(4_503_599_627_370_496.0),
        Choice(
            "A4 is read as its binary64 4503599627370495.5, a half rounded away from zero (no ROUND page example shows a half at 0 places); the 15-digit reading 4503599627370500 gives 4503599627370500"
        )
    );
    rnd!(
        r,
        s,
        "ROUND",
        [cell(s, "A5"), num("0")],
        N(-4_503_599_627_370_496.0),
        Choice(
            "A5 is read as its binary64 -4503599627370495.5, a half rounded away from zero (no ROUND page example shows a half at 0 places); the 15-digit reading gives -4503599627370500"
        )
    );
    rnd!(
        r,
        s,
        "ROUND",
        [cell(s, "A6"), num("0")],
        N(1_000_000_000_000_001.0),
        Choice(
            "A6 is read as its binary64 1000000000000000.5, a half rounded away from zero (no ROUND page example shows a half at 0 places); the 15-digit reading 1E15 gives 1E15"
        )
    );
    rnd!(
        r,
        s,
        "ROUND",
        [cell(s, "A1"), num("0")],
        N(9_007_199_254_740_992.0),
        Choice(
            "A1 is read as its binary64 2^53, an integer, unchanged; the 15-digit reading gives 9007199254740990"
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
        N(9_223_372_036_854_775_808.0),
        Choice(
            "A9 is read as its binary64 2^63, which rounds exactly to 9223372036854776000 and converts back to 2^63; the 15-digit reading 9223372036854780000 gives 9223372036854779904"
        )
    );
    rnd!(
        r,
        s,
        "ROUND",
        [cell(s, "A7"), num("0")],
        N(max),
        Choice(
            "A7 is read as the largest binary64, an integer, unchanged; the 15-digit reading lies beyond binary64's range and gives #NUM!"
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
        E(Num),
        Choice(OVERFLOW)
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
        N(1.5),
        Choice(
            "a num_digits of 1E300 passes through no integer type: no digit lies that far right"
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

    // MOD: the remainder is exact for every quotient. Excel's limit on the quotient is
    // undocumented, so every row here is an unmeasured choice; a row with a large quotient names
    // the no-limit choice.
    rnd!(
        r,
        s,
        "MOD",
        [cell(s, "A1"), num("3")],
        N(2.0),
        Choice(
            "no limit on the quotient, whose limit in Excel is undocumented: A1 is read as its binary64 2^53 = 3*3002399751580330 + 2 exactly, as n - d*INT(n/d) in binary64 steps also gives; the 15-digit reading 9007199254740990 gives 0"
        )
    );
    rnd!(
        r,
        s,
        "MOD",
        [cell(s, "A3"), num("3")],
        N(1.0),
        Choice(
            "no limit on the quotient, whose limit in Excel is undocumented: A3 is read as its binary64 2^53+2 = 3*3002399751580331 + 1 exactly; n - d*INT(n/d) in binary64 steps gives 2, and the 15-digit reading 9007199254740990 gives 0"
        )
    );
    rnd!(
        r,
        s,
        "MOD",
        [num("999999999999999"), num("2")],
        N(1.0),
        Choice(
            "no limit on the quotient, whose limit in Excel is undocumented: the quotient 499999999999999.5 leaves 1, the remainder with the divisor's sign"
        )
    );
    rnd!(
        r,
        s,
        "MOD",
        [num("1E15"), num("7")],
        N(6.0),
        Choice(
            "no limit on the quotient, whose limit in Excel is undocumented: 1E15 = 7*142857142857142 + 6, the remainder with the divisor's sign"
        )
    );
    rnd!(
        r,
        s,
        "MOD",
        [num("-1E15"), num("7")],
        N(1.0),
        Choice(
            "no limit on the quotient, whose limit in Excel is undocumented: -1E15 = 7*(-142857142857143) + 1, the remainder with the divisor's sign"
        )
    );
    rnd!(
        r,
        s,
        "MOD",
        [cell(s, "A10"), num("1024")],
        N(0.0),
        Choice(
            "no limit on the quotient, whose limit in Excel is undocumented: A10 is read as its binary64 2^60, a multiple of 1024; the 15-digit reading 1152921504606850000 gives 976"
        )
    );
    rnd!(
        r,
        s,
        "MOD",
        [cell(s, "A10"), num("3")],
        N(1.0),
        Choice(
            "no limit on the quotient, whose limit in Excel is undocumented: A10 is read as its binary64 2^60 = 3*384307168202282325 + 1 exactly (the 15-digit reading also leaves 1); n - d*INT(n/d) in binary64 steps gives 0"
        )
    );
    rnd!(
        r,
        s,
        "MOD",
        [num("1E300"), num("7")],
        N(1.0),
        Choice(
            "no limit on the quotient, whose limit in Excel is undocumented: 1E300 is read as the decimal it shows, 10^300, whose remainder by 7 is exactly 1"
        )
    );
    rnd!(
        r,
        s,
        "MOD",
        [num("1E23"), num("1E8")],
        N(0.0),
        Choice(
            "no limit on the quotient, whose limit in Excel is undocumented: 1E23 is read as the decimal it shows, 10^23, a multiple of 10^8, so 0; read as its binary64 99999999999999991611392 it leaves 91611392"
        )
    );
    rnd!(
        r,
        s,
        "MOD",
        [cell(s, "A7"), num("3")],
        N(2.0),
        Choice(
            "no limit on the quotient, whose limit in Excel is undocumented: the remainder of the largest binary64 by 3 is exactly 2, under either reading of A7"
        )
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
        [num("-5"), num("1E308")],
        N(1e308),
        Choice("1E308 - 5, the remainder with the divisor's sign, rounds once to 1E308")
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
        N(4_503_599_627_370_496.0),
        Choice(
            "the binary64 order: A4's binary64 4503599627370495.5 divided by 1 is itself, whose integer above is 4503599627370496, times 1; the 15-digit reading 4503599627370500 would give 4503599627370500"
        )
    );
    rnd!(
        r,
        s,
        "FLOOR",
        [cell(s, "A5"), num("1")],
        N(-4_503_599_627_370_496.0),
        Choice(
            "the binary64 order: A5's binary64 -4503599627370495.5 divided by 1 is itself, whose integer below, away from zero, is -4503599627370496, times 1; the 15-digit reading would give -4503599627370500"
        )
    );
    rnd!(
        r,
        s,
        "FLOOR",
        [cell(s, "A7"), num("1")],
        N(max),
        Choice(
            "the binary64 order: the largest binary64 divided by 1 is itself, an integer, and times 1 is A7 again, so an exact multiple is not rounded (documented); the 15-digit reading would lie beyond binary64's range and give #NUM!"
        )
    );
    rnd!(
        r,
        s,
        "FLOOR",
        [cell(s, "A10"), num("1024")],
        N(1_152_921_504_606_846_976.0),
        Choice(
            "the binary64 order: A10's binary64 2^60 divided by 1024 is 2^50 exactly, an integer, and 1024*2^50 is 2^60 again, so an exact multiple is not rounded (documented); the 15-digit reading 1152921504606850000 (binary64 1152921504606850048) would give 1152921504606850048"
        )
    );
    rnd!(
        r,
        s,
        "CEILING",
        [cell(s, "A7"), num("1E308")],
        E(Num),
        Choice(OVERFLOW)
    );
    rnd!(
        r,
        s,
        "FLOOR",
        [cell(s, "A8"), num("1E308")],
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
const DIGIT_LABEL: Owner = Owner {
    bead: "dc-e00t",
    behaviour: "the not-a-number verdict for digit-bearing text such as Q1 or 2016-17 Winter",
};
const SEVERAL_AREAS: Owner = Owner {
    bead: "dc-gdii",
    behaviour: "a range of several areas in the conditional aggregates",
};
const SHAPED_PAST: Owner = Owner {
    bead: "dc-1mc8",
    behaviour: "a SUMIF or AVERAGEIF sum range shaped past the reference the formula gives, as a recalculation prerequisite",
};
const NUMBER_JOINED: Owner = Owner {
    bead: "dc-q9rb",
    behaviour: "a number's text when & reads the number",
};

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
        "ACCEPT-NOTE: {}: {} rows through calc_func::invoke, each asserted against a value or a refusal derived here by hand and each with its formula on sheet {} of {CONDITIONAL_WORKBOOK_PATH}: {} from documented rules, {} unmeasured choices, {} refused (owned by: {}); none is compared with Excel here (cfunc_m3_5__excel_comparison compares them once that workbook is captured)",
        rows.case,
        rows.oracle.len(),
        rows.sheet,
        rows.oracle.len() - choices - refused,
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

/// A row of cfunc-m3.5 whose last argument is the criterion `"op"&A1` over a cell the & entry
/// refuses to read as text, so the formula's evaluation ends in that refusal before the call of
/// `spelling`: asserts the refusal, then records the & call and the row, refused and owned by
/// `owner`, with the next cell of column D, and prints it with `reason`.
#[allow(clippy::too_many_arguments)]
fn refused_join_row(
    rows: &mut Rows,
    sheet: &TestSheet,
    spelling: &str,
    before: &[Arg],
    op: &str,
    a1: &str,
    owner: Owner,
    reason: &str,
) {
    let concatenate = surface(Notation::Infix, "&");
    let tick = next_tick();
    let outcome = join(sheet, op, a1);
    let criterion = format!("\"{op}\"&{a1}");
    let mut texts: Vec<&str> = before.iter().map(|a| a.excel.as_str()).collect();
    texts.push(&criterion);
    let formula = surface(Notation::Call, spelling)
        .oracle_formula(&texts)
        .unwrap_or_else(|| panic!("{spelling} does not admit {texts:?}"));
    assert_eq!(
        outcome,
        Err(FunctionFault::NotImplemented {
            id: concatenate.id(),
            argument: Some(1)
        }),
        "{formula}: the & entry over {a1}"
    );
    let row = u32::try_from(rows.oracle.len() + 1).expect("a row number");
    if !rows.quiet {
        let mut reads: Vec<String> = before.iter().flat_map(|a| a.reads.clone()).collect();
        reads.extend(sheet.entries(&[a1]));
        note!(
            "ACCEPT-NOTE: {}: oracle {}!{} {formula} with {} -> & refuses: {:?} (refused as not modelled: {reason}; not compared with Excel)",
            rows.case,
            rows.sheet,
            encode_a1(row, FORMULA_COLUMN),
            reads.join(", "),
            outcome
        );
    }
    rows.calls.push(Call { row, tick });
    rows.oracle.push(OracleRow {
        row,
        formula,
        outcome,
        choice: None,
        owner: Some(owner),
        tick,
    });
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
const SHAPED_PAST_TEXT: &str = "the third argument shaped from its top-left cell to the range's size (the SUMIF and AVERAGEIF pages) extends past the reference the formula gives, whose cells past it the engine does not yet take as the formula's prerequisites; owned by dc-1mc8";
const SAME_SHAPE: &str =
    "every range has the same size and shape; positions align even when the ranges do not";
const SHAPE_VALUE: &str = "ranges of different sizes or shapes return #VALUE!, as the MAXIFS page states for MAXIFS; this page names no error";
const SKIPPED: &str =
    "text and logicals in the target are skipped, as SUM skips them in a reference";
const MATCHED_ERROR: &str =
    "an error in a matched target cell is the result, the first in row-major order";
const DIGIT_LABEL_TEXT: &str = "a criterion or an = criterion's range cell that holds a digit outside the plain decimal form, such as the label Q1 or 2016-17 Winter, is not modelled: whether Excel reads it as a number is owned by dc-e00t";
const NUMBER_TEXT_FORMS: &str = "a criterion that Excel's number-text rules may read as a number, such as the fraction or date 1/2, is not modelled; owned by cfml-m2.6";
const COLLATION_TEXT: &str = "text compared beyond the comparison operators' modelled rule needs Excel's collation; owned by cdoc-jpha";

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
            "a criterion may join an operator to a cell's value with &, as the page's example COUNTIF(B2:B5,\"<>\"&B4) does; calc_func's & entry joins =APPLE here, while a join over a number stays refused until dc-q9rb (row D91)"
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

    // COUNTIF: refused criteria and cells.
    cnd!(
        r,
        s,
        "COUNTIF",
        [a(s), txt("<b")],
        Refused(Some(0)),
        Unsupported(
            "\"Apple Pie\" ordered against b needs a space ordered; owned by cdoc-jpha (Excel's collation)"
        )
    );
    owned(r, COLLATION);
    cnd!(
        r,
        s,
        "COUNTIF",
        [a(s), txt("Q1")],
        Refused(Some(1)),
        Unsupported(DIGIT_LABEL_TEXT)
    );
    owned(r, DIGIT_LABEL);
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
        Refused(Some(0)),
        Unsupported(COLLATION_TEXT)
    );
    owned(r, COLLATION);
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
        Refused(Some(0)),
        Unsupported(
            "a pattern matched against text beyond ASCII needs Excel's collation; owned by cdoc-jpha"
        )
    );
    owned(r, COLLATION);
    cnd!(
        r,
        s,
        "COUNTIF",
        [reference(s, &["A24"]), num("0.3")],
        Refused(Some(0)),
        Unsupported(
            "0.30000000000000004 and 0.3 lie within a relative 1e-13; Excel's comparison of nearly equal numbers is owned by cdoc-jpha"
        )
    );
    owned(r, NEAR_EQUAL);
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
        Refused(Some(2)),
        Unsupported(SHAPED_PAST_TEXT)
    );
    owned(r, SHAPED_PAST);
    cnd!(
        r,
        s,
        "SUMIF",
        [c(s), txt("<3"), reference(s, &["B5:B6"])],
        Refused(Some(2)),
        Unsupported(SHAPED_PAST_TEXT)
    );
    owned(r, SHAPED_PAST);
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
        Refused(Some(2)),
        Unsupported(DIGIT_LABEL_TEXT)
    );
    owned(r, DIGIT_LABEL);
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
        Refused(Some(0)),
        Unsupported(DIGIT_LABEL_TEXT)
    );
    owned(r, DIGIT_LABEL);

    // A criterion joined over a number: & refuses the number's text, so the formula ends there.
    refused_join_row(
        r,
        s,
        "COUNTIF",
        &[c(s)],
        "=",
        "C4",
        NUMBER_JOINED,
        "& reads the number 4 as text, which calc_func refuses until dc-q9rb gives a number's text as Excel does",
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
        Choice(
            "the shapes are compared before any criterion is read, so a criterion that is not modelled does not refuse the call",
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

    // SUMIF and AVERAGEIF shape their third argument from its top-left cell. A shaped range
    // inside the reference the formula gives is read; one that extends past it is refused, the
    // engine not yet taking the cells past it as the formula's prerequisites (dc-1mc8).
    measure(
        r,
        s,
        "SUMIF",
        vec![at(s, &["B3:B6"]), txt("a"), at(s, &["A2"])],
        &[],
        Refused(Some(2)),
        Unsupported(SHAPED_PAST_TEXT),
    );
    owned(r, SHAPED_PAST);
    measure(
        r,
        s,
        "SUMIF",
        vec![at(s, &["B3:B6"]), txt("a"), at(s, &["A2:A3"])],
        &[],
        Refused(Some(2)),
        Unsupported(SHAPED_PAST_TEXT),
    );
    owned(r, SHAPED_PAST);
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
        &[],
        Refused(Some(2)),
        Unsupported(SHAPED_PAST_TEXT),
    );
    owned(r, SHAPED_PAST);
    measure(
        r,
        s,
        "AVERAGEIF",
        vec![at(s, &["B3:B6"]), txt("b"), at(s, &["A2:B2"])],
        &[],
        Refused(Some(2)),
        Unsupported(SHAPED_PAST_TEXT),
    );
    owned(r, SHAPED_PAST);
    measure(
        r,
        s,
        "SUMIF",
        vec![at(s, &["B3:C4"]), txt("a"), at(s, &["A2"])],
        &[],
        Refused(Some(2)),
        Unsupported(SHAPED_PAST_TEXT),
    );
    owned(r, SHAPED_PAST);
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
        &[],
        Refused(Some(2)),
        Unsupported(
            "the sum_range shaped from A1048575 to 4 rows extends past A1048575 and past the sheet's last row; owned by dc-1mc8 (Excel's ad-hoc read gives 7 for it and for SUMIF(B3:B6,\"a\",A1048575:A1048576), as if the shaped range were cut at the last row)",
        ),
    );
    owned(r, SHAPED_PAST);
    measure(
        r,
        s,
        "SUMIF",
        vec![at(s, &["B3:B4", "B6"]), txt("a")],
        &[],
        Refused(Some(0)),
        Unsupported("a range of several areas is not modelled"),
    );
    owned(r, SEVERAL_AREAS);

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
        &[],
        Refused(Some(2)),
        Unsupported(SHAPED_PAST_TEXT),
    );
    owned(r, SHAPED_PAST);
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
    assert_eq!(
        capture.source, workbook,
        "the capture's source is not the conditional oracle workbook as committed"
    );
    assert_eq!(
        session.input_package, workbook,
        "the capture's input package is not the conditional oracle workbook as committed"
    );
    let worker = format!("worker sha256:{}", sha256_hex(CAPTURE_WORKER));
    assert!(
        session.tool.split("; ").any(|part| part == worker),
        "the capture-tool revision {:?} does not name the committed worker ({worker})",
        session.tool
    );
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
