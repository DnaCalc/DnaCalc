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
    // arity no argument is asked for. An eager entry is asked for each argument once, in order;
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
            let in_order: Vec<usize> = (0..arity.min).collect();
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
const GENERATED_WORKBOOKS: [&str; 2] = ["cfunc_m1_2/oracle.xlsx", "cfunc_m3_4/oracle.xlsx"];

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
        .map(|row| ((row.row, FORMULA_COLUMN), row.formula[1..].to_owned()))
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
    let parts = stored_parts(ORACLE_WORKBOOK);
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
    let source = format!("sha256:{}", sha256_hex(ORACLE_WORKBOOK));
    let context = workbook_context().fingerprint();
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
            case: CAPTURE_CASE.to_owned(),
            source: source.clone(),
            run: RunIdentity {
                revision: AUTHORED_REVISION,
                context,
            },
            results,
        },
        EvaluationRecord {
            case: CAPTURE_CASE.to_owned(),
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

/// What on `report` disagrees with the rows of `case`'s sheet: an inventory cell of the sheet that
/// no row has, a row of cfunc-m1.2 whose verdict is not `matched`, a row of another bead whose
/// verdict is not `not_modeled` with its owner, and a row whose cell the inventory lacks.
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
                "{at} {formula}: a row of cfunc-m1.2 is {}, not matched: {verdict:?}",
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
    let source = format!("sha256:{}", sha256_hex(ORACLE_WORKBOOK));
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
        case: CAPTURE_CASE.to_owned(),
        source: source.clone(),
        acquisition: Acquisition::LiveCapture,
        environment: None,
        sheets,
        formulas,
        observations,
        session: Some(CaptureSession {
            revision: AUTHORED_REVISION,
            operations: Vec::new(),
            context: workbook_context().fingerprint(),
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

/// Runs the named call `surface` on `args` through calc_func::invoke under Fast with the test
/// sheet as the cell reader and asserts `expect`, that the eager call asked for each argument once
/// in order, and that the result fits `basis` (a value, or a refusal for an unsupported row). Then
/// records the row with the next cell of column D and prints it with its basis.
fn aggregate_row(
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
            "{formula}: a row of cfunc-m3.4 computes a value"
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
