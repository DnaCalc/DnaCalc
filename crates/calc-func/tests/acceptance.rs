//! Acceptance cases owned by the calc-func package, each registered by its exact name in
//! `crates/calc-func/acceptance.json`. Every case is `#[test]` natively and `#[wasm_bindgen_test]`
//! on wasm32, with no browser or Node.js configuration of its own: `cargo xtask accept` forces the
//! browser with `WASM_BINDGEN_USE_BROWSER`. The cases reach the value types only through
//! calc-func's re-export `calc_func::value`.
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
    ArgumentDemand, EvalServices, FunctionCallScratch, FunctionCallTarget, FunctionFault,
    FunctionId, Notation, ParameterClass, PreparedArguments, RegistrationError, catalogue, invoke,
};
use std::alloc::{GlobalAlloc, Layout, System};
use std::any::Any;
use std::cell::Cell;
use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::hash::{BuildHasherDefault, DefaultHasher};
use std::hint::black_box;
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
}

struct CountingAllocator;

fn count_allocation() {
    let _ = ALLOCATIONS.try_with(|count| count.set(count.get() + 1));
}

// SAFETY: every method forwards to `System` with the caller's arguments unchanged.
unsafe impl GlobalAlloc for CountingAllocator {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        count_allocation();
        // SAFETY: forwarded unchanged; the caller upholds `GlobalAlloc::alloc`'s contract.
        unsafe { System.alloc(layout) }
    }

    unsafe fn alloc_zeroed(&self, layout: Layout) -> *mut u8 {
        count_allocation();
        // SAFETY: forwarded unchanged; the caller upholds `GlobalAlloc::alloc_zeroed`'s contract.
        unsafe { System.alloc_zeroed(layout) }
    }

    unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, new_size: usize) -> *mut u8 {
        count_allocation();
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
/// arithmetic, comparison and concatenation operators, SUM and IF.
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

/// One call through `calc_func::invoke` with fresh services and scratch.
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

    // Names: the surface table holds exactly the admitted surfaces, and each surface and each ID
    // resolves to the same entry; a named call also in lower case.
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
    assert_eq!(surfaces, expected, "the admitted surfaces");

    // Arity and dispatch: every ID of the dispatch table, invoked with its arity's minimum,
    // reaches its own entry; outside its arity no argument is asked for. Only SUM computes, only
    // IF is lazy, and the eager entries are asked for each argument once, in order.
    let mut arity_ids = BTreeSet::new();
    let mut computed = Vec::new();
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
            Err(fault) => assert_eq!(
                fault,
                FunctionFault::NotImplemented {
                    id: target.id(),
                    argument: None
                },
                "{target:?}"
            ),
        }
        let in_order: Vec<usize> = (0..arity.min).collect();
        if demand.is_lazy() {
            lazy.push(target.id());
            assert!(
                at_minimum.asked.is_empty(),
                "{target:?} asked {:?}",
                at_minimum.asked
            );
        } else {
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
    assert_eq!(dispatch_ids.len(), ADMITTED.len());

    let sum = surface(Notation::Call, "SUM");
    let iff = surface(Notation::Call, "IF");
    assert_eq!(
        computed,
        [(sum.id(), CalcValue::number(1.0))],
        "only SUM computes"
    );
    assert_eq!(lazy, [iff.id()], "only IF is lazy");
    note!(
        "ACCEPT-NOTE: {CASE}: {} admitted IDs; the dispatch, name, help, arity and oracle views each enumerate the same {}; computing entries {:?}; lazy entries {:?}",
        dispatch_ids.len(),
        dispatch_ids.len(),
        computed.iter().map(|(id, _)| *id).collect::<Vec<_>>(),
        lazy
    );

    // SUM(1, 2, 3) through calc_func::invoke, by FunctionCallTarget::invoke.
    let mut one_two_three = Recorded::numbers(3);
    let six = sum.invoke(
        &mut one_two_three,
        &mut EvalServices::new(&fast),
        &mut FunctionCallScratch::new(),
    );
    assert_eq!(
        six.as_ref()
            .map(|value| value.as_number().map(f64::to_bits)),
        Ok(Some(6.0f64.to_bits()))
    );
    assert_eq!(one_two_three.asked, [0, 1, 2]);
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
        "ACCEPT-NOTE: {CASE}: SUM(1,2,3) through calc_func::invoke = {:?} (bits {:#018x}); arguments asked {:?}; allocation calls: {first_allocations} on fresh scratch, {second_allocations} on warm scratch",
        six.as_ref().ok().and_then(CalcValue::as_number),
        6.0f64.to_bits(),
        one_two_three.asked
    );

    // Under Exact, SUM refuses instead of adding under Fast.
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

    // IF's demand holds its branches back, and the call interface asks for none of its
    // arguments; the eager "=" is asked for both of the same kind of arguments.
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
        Err(FunctionFault::NotImplemented {
            id: iff.id(),
            argument: None
        })
    );
    assert!(branches.asked.is_empty());
    let equal = surface(Notation::Infix, "=");
    assert_eq!(equal.argument_demand(), ArgumentDemand::Eager);
    let mut operands = Recorded::new(vec![
        CalcValue::logical(true),
        CalcValue::error(WorksheetErrorCode::Div0),
    ]);
    assert_eq!(
        run(equal, &mut operands, &fast),
        Err(FunctionFault::NotImplemented {
            id: equal.id(),
            argument: None
        })
    );
    assert_eq!(operands.asked, [0, 1]);
    note!(
        "ACCEPT-NOTE: {CASE}: IF demand {demand:?}, discriminator {:?}; invoke asked for {:?} of IF's 3 arguments and {:?} of the 2 arguments of the eager = operator",
        demand.discriminator(),
        branches.asked,
        operands.asked
    );

    // Parameter classes: an operator's value parameter refuses a reference; SUM's and IF's
    // branch parameters admit one, and SUM's kernel says it does not read references yet.
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
    let mut sum_reference = Recorded::new(vec![CalcValue::number(1.0), a1_reference()]);
    assert_eq!(
        run(sum, &mut sum_reference, &fast),
        Err(FunctionFault::NotImplemented {
            id: sum.id(),
            argument: Some(1)
        })
    );
    // A direct array passes admission for a value parameter and for SUM's, and reaches SUM's
    // kernel, which does not read arrays yet.
    let array = CalcValue::array(CalcArray::from_scalar(CalcValue::number(2.0)));
    let mut plus_array = Recorded::new(vec![CalcValue::number(1.0), array.clone()]);
    assert_eq!(
        run(plus, &mut plus_array, &fast),
        Err(FunctionFault::NotImplemented {
            id: plus.id(),
            argument: None
        })
    );
    let mut sum_array = Recorded::new(vec![array, CalcValue::number(1.0)]);
    assert_eq!(
        run(sum, &mut sum_array, &fast),
        Err(FunctionFault::NotImplemented {
            id: sum.id(),
            argument: Some(0)
        })
    );

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

/// calc-func's source files, read at compile time so the scan also runs in the browsers: the
/// crate root, the modules it declares and the family fragments the catalogue declares. The case
/// checks that this set equals the declared modules.
const CALC_FUNC_SOURCES: [(&str, &str); 8] = [
    ("lib", include_str!("../src/lib.rs")),
    ("call", include_str!("../src/call.rs")),
    ("catalogue", include_str!("../src/catalogue.rs")),
    (
        "catalogue/aggregate",
        include_str!("../src/catalogue/aggregate.rs"),
    ),
    (
        "catalogue/arithmetic",
        include_str!("../src/catalogue/arithmetic.rs"),
    ),
    (
        "catalogue/comparison",
        include_str!("../src/catalogue/comparison.rs"),
    ),
    (
        "catalogue/logical",
        include_str!("../src/catalogue/logical.rs"),
    ),
    ("catalogue/text", include_str!("../src/catalogue/text.rs")),
];

/// The string literals of Rust source text outside its comments, each as written between its
/// quotes. Char literals and lifetimes are skipped.
fn string_literals(text: &str) -> Vec<&str> {
    let bytes = text.as_bytes();
    let word = |b: u8| b.is_ascii_alphanumeric() || b == b'_';
    let mut found = Vec::new();
    let mut at = 0;
    while at < bytes.len() {
        match bytes[at] {
            b'/' if bytes.get(at + 1) == Some(&b'/') => {
                while at < bytes.len() && bytes[at] != b'\n' {
                    at += 1;
                }
            }
            b'/' if bytes.get(at + 1) == Some(&b'*') => {
                let end = text[at + 2..]
                    .find("*/")
                    .expect("an unterminated block comment");
                at += end + 4;
            }
            b'r' if (at == 0 || !word(bytes[at - 1]))
                && matches!(bytes.get(at + 1), Some(b'"' | b'#')) =>
            {
                let mut quote = at + 1;
                while bytes.get(quote) == Some(&b'#') {
                    quote += 1;
                }
                if bytes.get(quote) != Some(&b'"') {
                    at += 1;
                    continue;
                }
                let closing = format!("\"{}", "#".repeat(quote - at - 1));
                let start = quote + 1;
                let end = start
                    + text[start..]
                        .find(&closing)
                        .expect("an unterminated raw string");
                found.push(&text[start..end]);
                at = end + closing.len();
            }
            b'"' => {
                let start = at + 1;
                let mut end = start;
                while bytes[end] != b'"' {
                    if bytes[end] == b'\\' {
                        end += 1;
                    }
                    end += 1;
                }
                found.push(&text[start..end]);
                at = end + 1;
            }
            b'\'' => {
                if bytes.get(at + 1) == Some(&b'\\') {
                    let mut end = at + 3;
                    while bytes[end] != b'\'' {
                        end += 1;
                    }
                    at = end + 1;
                } else {
                    let after = text[at + 1..]
                        .chars()
                        .next()
                        .map_or(at + 1, |c| at + 1 + c.len_utf8());
                    at = if bytes.get(after) == Some(&b'\'') {
                        after + 1
                    } else {
                        at + 1
                    };
                }
            }
            _ => at += 1,
        }
    }
    found
}

#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test)]
fn cfunc_m1_1__unknown_function() {
    const CASE: &str = "cfunc_m1_1__unknown_function";
    let catalogue = catalogue();
    let admitted: BTreeSet<u32> = catalogue.targets().map(|t| t.id().raw()).collect();
    assert_eq!(admitted.len(), ADMITTED.len());

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

    // No second string switch: the files read are exactly the crate root, its modules and the
    // catalogue's fragments, and across them each admitted spelling occurs as a string literal
    // exactly as often as catalogue entries declare it, so no other code matches on a name or a
    // token.
    let declared_by = |name: &str| {
        let text = CALC_FUNC_SOURCES
            .iter()
            .find(|(module, _)| *module == name)
            .map(|(_, text)| *text)
            .unwrap_or_else(|| panic!("{name} was not read"));
        let mut modules = out_of_line_modules(text);
        modules.sort_unstable();
        modules
    };
    assert_eq!(declared_by("lib"), ["call", "catalogue"]);
    assert_eq!(
        declared_by("catalogue"),
        ["aggregate", "arithmetic", "comparison", "logical", "text"]
    );
    for (module, text) in CALC_FUNC_SOURCES {
        assert!(!text.is_empty(), "{module} was not read");
        if module != "lib" && module != "catalogue" {
            assert!(
                out_of_line_modules(text).is_empty(),
                "{module} declares a module this case does not read"
            );
        }
    }
    let literals: Vec<&str> = CALC_FUNC_SOURCES
        .iter()
        .flat_map(|(_, text)| string_literals(text))
        .collect();
    let mut declared: BTreeMap<String, usize> = BTreeMap::new();
    for target in catalogue.surfaces() {
        *declared
            .entry(target.spelling().to_ascii_uppercase())
            .or_default() += 1;
    }
    for (spelling, entries) in &declared {
        let occurrences = literals
            .iter()
            .filter(|literal| literal.eq_ignore_ascii_case(spelling))
            .count();
        assert_eq!(
            occurrences, *entries,
            "{spelling:?} occurs {occurrences} times as a string literal in calc-func's sources, but {entries} catalogue entries declare it"
        );
    }
    note!(
        "ACCEPT-NOTE: {CASE}: scanned {} string literals in {} calc-func source files; each of the {} distinct admitted spellings occurs exactly as often as entries declare it ({} declarations)",
        literals.len(),
        CALC_FUNC_SOURCES.len(),
        declared.len(),
        declared.values().sum::<usize>()
    );
}
