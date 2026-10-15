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
use std::alloc::{GlobalAlloc, Layout, System};
use std::any::Any;
use std::cell::Cell;
use std::collections::{BTreeMap, HashMap};
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
