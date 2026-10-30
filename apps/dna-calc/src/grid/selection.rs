//! The grid skin's select and navigate grammar, with no DOM and no Leptos.
//!
//! 1. **The key grammar**: one declarative table ([`sheet_keymap`]) from a normalized
//!    [`KeyChord`] to a [`SheetAction`], with [`resolve_action`]. The grammar is data an atlas of
//!    key bindings can list, never scattered `match` arms on key names.
//! 2. **The movement**: [`next_active`], the edge-clamped map from an active cell and a
//!    [`NavAction`] to the next active cell over a [`GridExtent`]. Navigation stops at the four
//!    edges and never wraps.
//!
//! The canonical host selection keeps its active cell separately from its rectangles. Sparse
//! occupancy drives data-edge moves; geometry drives hidden-axis and page movement. No operation
//! expands a whole row, column or selection into its blank cells.
//!
//! Ported from DnaTreeCalc (commit bf6dfd7) `src/dnacalc-stage-sheet/src/selection.rs`, with its
//! tests. Delete clears the selection ([`SheetAction::Clear`]) and Shift+Arrow extends it
//! ([`SheetAction::Extend`], dc-09v); the legacy reasons for what the interaction pack had not
//! built are left behind with the actions that needed them.

/// The 1-based extent of the grid the active cell moves over: `rows`/`cols` are
/// the last valid row/column (Excel-style inclusive maxima), so a legal cell is
/// any `(r, c)` with `1 <= r <= rows` and `1 <= c <= cols`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct GridExtent {
    pub rows: u32,
    pub cols: u32,
}

impl GridExtent {
    /// Build an extent, flooring each dimension at 1 so there is always at least
    /// one legal cell (a degenerate `0` extent would leave [`next_active`] with an
    /// empty clamp range). A grid the stage draws always has a real extent
    /// (`max_rows`/`max_cols` from the projection), so this floor is a guard, not
    /// a normal path.
    #[must_use]
    pub fn new(rows: u32, cols: u32) -> Self {
        Self {
            rows: rows.max(1),
            cols: cols.max(1),
        }
    }

    /// Clamp a candidate `(row, col)` into `[1, rows] × [1, cols]`. Both axes are
    /// clamped independently, so an off-edge move parks the active cell on the
    /// nearest edge cell rather than wrapping or leaving the grid.
    #[must_use]
    pub fn clamp(self, row: u32, col: u32) -> (u32, u32) {
        (row.clamp(1, self.rows), col.clamp(1, self.cols))
    }

    /// [`GridExtent::clamp`] on a `(row, col)` pair — the shape the wiring holds an
    /// active cell in.
    #[must_use]
    pub fn clamp_pair(self, cell: (u32, u32)) -> (u32, u32) {
        self.clamp(cell.0, cell.1)
    }
}

/// A single-cell move over the grid. This is the movement half of the model —
/// distinct from [`SheetAction`], which also carries the mode-change and degrade
/// verbs the keyboard grammar resolves. Every variant lands on exactly one cell
/// (single-cell selection), never a range.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub enum NavAction {
    /// One row up (Arrow Up).
    Up,
    /// One row down (Arrow Down).
    Down,
    /// One column left (Arrow Left).
    Left,
    /// One column right (Arrow Right).
    Right,
    /// One column right (Tab).
    NextColumn,
    /// One column left (Shift+Tab).
    PrevColumn,
    /// The first column of the current row (Home).
    RowStart,
    /// The top-left cell `(1, 1)` (Ctrl+Home).
    GridStart,
    /// One row down, keeping the column — the move an accepted Enter-commit makes
    /// (Excel advances down after committing).
    CommitDown,
    /// One column right, keeping the row — the move an accepted Tab-commit makes
    /// (Excel advances right after committing with Tab). The horizontal twin of
    /// [`NavAction::CommitDown`].
    CommitRight,
}

/// The next active cell after applying `action` to `current` over `extent`.
///
/// Pure and total: every move is computed with [`u32::saturating_sub`] on the low
/// edge and [`GridExtent::clamp`] on the high edge, so the result is always a
/// legal in-extent cell and the function never underflows, overflows, or panics.
/// Clamping is per-axis with NO wrap — a move off the top/bottom/left/right edge
/// parks on that edge (documented invariant, asserted by the tests).
#[must_use]
pub fn next_active(current: (u32, u32), action: NavAction, extent: GridExtent) -> (u32, u32) {
    let (row, col) = current;
    let (next_row, next_col) = match action {
        NavAction::Up => (row.saturating_sub(1).max(1), col),
        NavAction::Down => (row.saturating_add(1), col),
        NavAction::Left => (row, col.saturating_sub(1).max(1)),
        NavAction::Right => (row, col.saturating_add(1)),
        NavAction::NextColumn => (row, col.saturating_add(1)),
        NavAction::PrevColumn => (row, col.saturating_sub(1).max(1)),
        NavAction::RowStart => (row, 1),
        NavAction::GridStart => (1, 1),
        NavAction::CommitDown => (row.saturating_add(1), col),
        NavAction::CommitRight => (row, col.saturating_add(1)),
    };
    extent.clamp(next_row, next_col)
}

/// Commands whose result depends on the workbook selection, occupancy or viewport.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub enum Navigation {
    Step(NavAction),
    Edge(NavAction),
    UsedEnd,
    Page { down: bool },
    Walk { horizontal: bool, reverse: bool },
    Column,
    Row,
    All,
}

use super::geometry::GridMetrics;
use calc_host::Selection;
use calc_host::value::{CellKey, GridArea, MAX_COLUMNS, MAX_ROWS};

/// The next boundary of a sparse occupied line. Empty formula results still occupy cells.
pub fn data_edge(current: u32, forward: bool, limit: u32, intervals: &[(u32, u32)]) -> u32 {
    let mut runs = intervals.to_vec();
    runs.sort_unstable();
    let mut merged: Vec<(u32, u32)> = Vec::new();
    for (first, last) in runs {
        if let Some(run) = merged.last_mut()
            && first <= run.1.saturating_add(1)
        {
            run.1 = run.1.max(last);
        } else {
            merged.push((first, last));
        }
    }
    if let Some(&(first, last)) = merged
        .iter()
        .find(|&&(first, last)| first <= current && current <= last)
    {
        if forward && current < last {
            return last;
        }
        if !forward && current > first {
            return first;
        }
    }
    if forward {
        merged
            .iter()
            .find(|&&(first, _)| first > current)
            .map_or(limit, |r| r.0)
    } else {
        merged
            .iter()
            .rev()
            .find(|&&(_, last)| last < current)
            .map_or(1, |r| r.1)
    }
}

fn rectangle(at: CellKey, first: (u32, u32), last: (u32, u32)) -> GridArea {
    GridArea::new(
        CellKey::new(at.sheet(), first.0, first.1).expect("bounded coordinate"),
        CellKey::new(at.sheet(), last.0, last.1).expect("bounded coordinate"),
    )
    .expect("one sheet")
}

pub fn current_region(at: CellKey, occupied: &[GridArea]) -> Option<GridArea> {
    if !occupied.iter().any(|a| a.contains(at)) {
        return None;
    }
    let mut region = GridArea::cell(at);
    loop {
        let old = region;
        for area in occupied.iter().filter(|a| a.sheet() == at.sheet()) {
            if area.first().row() <= region.last().row().saturating_add(1)
                && area.last().row().saturating_add(1) >= region.first().row()
                && area.first().column() <= region.last().column().saturating_add(1)
                && area.last().column().saturating_add(1) >= region.first().column()
            {
                region = rectangle(
                    at,
                    (
                        region.first().row().min(area.first().row()),
                        region.first().column().min(area.first().column()),
                    ),
                    (
                        region.last().row().max(area.last().row()),
                        region.last().column().max(area.last().column()),
                    ),
                );
            }
        }
        if region == old {
            return Some(region);
        }
    }
}

/// The corner opposite an active corner, independently along each axis.
pub fn opposite(area: GridArea, at: CellKey) -> (u32, u32) {
    let (first, last) = (area.first(), area.last());
    let row = if at.row() == first.row() {
        last.row()
    } else {
        first.row()
    };
    let column = if at.column() == first.column() {
        last.column()
    } else {
        first.column()
    };
    (row, column)
}

/// Work out a complete selection without enumerating any blank row, column or rectangle.
pub fn navigate(
    selection: &Selection,
    nav: Navigation,
    extend: bool,
    metrics: &GridMetrics,
    page_height: f64,
    occupied: &[GridArea],
    used: Option<GridArea>,
) -> Selection {
    let Some(at) = selection.active else {
        return selection.clone();
    };
    let area = selection
        .areas
        .first()
        .copied()
        .unwrap_or(GridArea::cell(at));
    let single = |active: CellKey, area| Selection {
        active: Some(active),
        areas: vec![area],
    };
    match nav {
        Navigation::Column => {
            return single(
                at,
                rectangle(
                    at,
                    (1, area.first().column()),
                    (MAX_ROWS, area.last().column()),
                ),
            );
        }
        Navigation::Row => {
            return single(
                at,
                rectangle(
                    at,
                    (area.first().row(), 1),
                    (area.last().row(), MAX_COLUMNS),
                ),
            );
        }
        Navigation::All => {
            let region = current_region(at, occupied);
            let target = match region {
                Some(region)
                    if selection.areas.as_slice() != [region]
                        && selection.areas.as_slice() != [GridArea::full_sheet(at.sheet())] =>
                {
                    region
                }
                _ => GridArea::full_sheet(at.sheet()),
            };
            return single(at, target);
        }
        Navigation::Walk {
            horizontal,
            reverse,
        } if selection.areas.len() > 1 || area.cell_count() > 1 => {
            let mut index = selection
                .areas
                .iter()
                .position(|a| a.contains(at))
                .unwrap_or(0);
            let area = selection.areas[index];
            let step = |n: u32, low: u32, high: u32, rows: bool| -> Option<u32> {
                let next = if reverse {
                    n.checked_sub(1)?
                } else {
                    n.checked_add(1)?
                };
                let visible = if rows {
                    metrics.visible_row(next.max(1), !reverse, MAX_ROWS)
                } else {
                    metrics.visible_col(next.max(1), !reverse, MAX_COLUMNS)
                };
                (low <= visible && visible <= high && visible != n).then_some(visible)
            };
            let (row, col) = (at.row(), at.column());
            let first_row = if reverse {
                area.last().row()
            } else {
                area.first().row()
            };
            let first_col = if reverse {
                area.last().column()
            } else {
                area.first().column()
            };
            let next = if horizontal {
                step(col, area.first().column(), area.last().column(), false)
                    .map(|c| (row, c))
                    .or_else(|| {
                        step(row, area.first().row(), area.last().row(), true)
                            .map(|r| (r, first_col))
                    })
            } else {
                step(row, area.first().row(), area.last().row(), true)
                    .map(|r| (r, col))
                    .or_else(|| {
                        step(col, area.first().column(), area.last().column(), false)
                            .map(|c| (first_row, c))
                    })
            };
            let visible = |area: GridArea, (row, col): (u32, u32)| {
                let row = metrics.visible_row(row, !reverse, MAX_ROWS);
                let col = metrics.visible_col(col, !reverse, MAX_COLUMNS);
                let target = CellKey::new(area.sheet(), row, col).ok()?;
                area.contains(target).then_some(target)
            };
            let mut target = next.and_then(|next| visible(area, next));
            if target.is_none() {
                for _ in 0..selection.areas.len() {
                    index = if reverse {
                        (index + selection.areas.len() - 1) % selection.areas.len()
                    } else {
                        (index + 1) % selection.areas.len()
                    };
                    let area = selection.areas[index];
                    let corner = if reverse { area.last() } else { area.first() };
                    target = visible(area, (corner.row(), corner.column()));
                    if target.is_some() {
                        break;
                    }
                }
            }
            return target.map_or_else(
                || selection.clone(),
                |target| Selection {
                    active: Some(target),
                    areas: selection.areas.clone(),
                },
            );
        }
        _ => {}
    }
    let from = if extend {
        let (row, col) = opposite(area, at);
        CellKey::new(at.sheet(), row, col).expect("area corner")
    } else {
        at
    };
    let (row, col) = (from.row(), from.column());
    let next = match nav {
        Navigation::Step(action) => {
            next_active((row, col), action, GridExtent::new(MAX_ROWS, MAX_COLUMNS))
        }
        Navigation::Walk {
            horizontal,
            reverse,
        } => next_active(
            (row, col),
            match (horizontal, reverse) {
                (true, false) => NavAction::Right,
                (true, true) => NavAction::Left,
                (false, false) => NavAction::Down,
                (false, true) => NavAction::Up,
            },
            GridExtent::new(MAX_ROWS, MAX_COLUMNS),
        ),
        Navigation::Edge(direction) => {
            let horizontal = matches!(direction, NavAction::Left | NavAction::Right);
            let forward = matches!(direction, NavAction::Right | NavAction::Down);
            let intervals: Vec<_> = occupied
                .iter()
                .filter(|a| a.sheet() == at.sheet())
                .filter_map(|a| {
                    if horizontal && a.first().row() <= row && row <= a.last().row() {
                        Some((a.first().column(), a.last().column()))
                    } else if !horizontal && a.first().column() <= col && col <= a.last().column() {
                        Some((a.first().row(), a.last().row()))
                    } else {
                        None
                    }
                })
                .collect();
            if horizontal {
                (row, data_edge(col, forward, MAX_COLUMNS, &intervals))
            } else {
                (data_edge(row, forward, MAX_ROWS, &intervals), col)
            }
        }
        Navigation::UsedEnd => used.map_or((1, 1), |a| (a.last().row(), a.last().column())),
        Navigation::Page { down } => {
            let distance = (page_height - metrics.header_h).max(metrics.height(row));
            let offset =
                (metrics.row_start(row) + if down { distance } else { -distance }).max(0.0);
            let mut target = metrics.row_at(offset, MAX_ROWS);
            if !down && metrics.row_start(target) < offset {
                target = target.saturating_add(1).min(MAX_ROWS);
            }
            (target, col)
        }
        _ => (row, col),
    };
    let home = matches!(
        nav,
        Navigation::Step(NavAction::RowStart | NavAction::GridStart)
    );
    let next = (
        metrics.visible_row(next.0, home || next.0 >= row, MAX_ROWS),
        metrics.visible_col(next.1, home || next.1 >= col, MAX_COLUMNS),
    );
    let target = CellKey::new(at.sheet(), next.0, next.1).expect("bounded navigation");
    if extend {
        single(at, GridArea::new(at, target).expect("one sheet"))
    } else {
        single(target, GridArea::cell(target))
    }
}

/// Remove a selected rectangle using at most four rectangles per existing area.
pub fn subtract(area: GridArea, removed: GridArea) -> Vec<GridArea> {
    if area.sheet() != removed.sheet() {
        return vec![area];
    }
    let (t, l, b, r) = (
        area.first().row().max(removed.first().row()),
        area.first().column().max(removed.first().column()),
        area.last().row().min(removed.last().row()),
        area.last().column().min(removed.last().column()),
    );
    if t > b || l > r {
        return vec![area];
    }
    let mut result = Vec::new();
    let at = area.first();
    if area.first().row() < t {
        result.push(rectangle(
            at,
            (area.first().row(), area.first().column()),
            (t - 1, area.last().column()),
        ));
    }
    if b < area.last().row() {
        result.push(rectangle(
            at,
            (b + 1, area.first().column()),
            (area.last().row(), area.last().column()),
        ));
    }
    if area.first().column() < l {
        result.push(rectangle(at, (t, area.first().column()), (b, l - 1)));
    }
    if r < area.last().column() {
        result.push(rectangle(at, (t, r + 1), (b, area.last().column())));
    }
    result
}

// --- Keyboard grammar (mirrors the Notebook stage's keyboard.rs) -------------

/// Normalize a raw DOM `KeyboardEvent.key` string into the table's vocabulary: a
/// single alphabetic key is lowercased so its `Shift` state is carried ONLY by
/// [`KeyChord::shift`] (never by the key's case), while every multi-character key
/// (`"ArrowUp"`, `"Tab"`, `"Home"`, `"F2"`, `"Enter"`, `"Escape"`) passes through
/// unchanged. Mirrors the Notebook grammar's `normalize_key`.
fn normalize_key(key: &str) -> String {
    let mut chars = key.chars();
    match (chars.next(), chars.next()) {
        (Some(ch), None) if ch.is_alphabetic() => ch.to_ascii_lowercase().to_string(),
        _ => key.to_string(),
    }
}

/// A normalized key chord: a key name plus the three modifier flags the Sheet
/// grammar distinguishes. The platform `meta` key (macOS `Cmd`) is folded into
/// `ctrl` by the wiring before a chord is built (estate convention), so a chord is
/// layout-portable.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct KeyChord {
    pub key: String,
    pub shift: bool,
    pub ctrl: bool,
    pub alt: bool,
}

impl KeyChord {
    /// Build a chord from raw parts, normalizing the key name. `ctrl` should
    /// already fold the platform `meta` key in (the wiring does this).
    #[must_use]
    pub fn new(key: impl AsRef<str>, shift: bool, ctrl: bool, alt: bool) -> Self {
        Self {
            key: normalize_key(key.as_ref()),
            shift,
            ctrl,
            alt,
        }
    }
}

/// The Sheet stage's closed set of keyboard actions. A binding row targets exactly
/// one of these.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SheetAction {
    /// Move the active cell one step (Arrows / Tab / Shift+Tab / Home /
    /// Ctrl+Home). Carries the [`NavAction`] so the wiring calls [`next_active`]
    /// with it directly — the movement grammar and the key grammar cannot drift.
    Move(NavAction),
    Navigate(Navigation, bool),
    /// Open the one overlay editor at the active cell, seeded with the cell's
    /// EXISTING authored text (F2 edit-in-place, or a double-click). Type-to-replace
    /// is NOT a keymap row — it is any printable key, handled as a wiring fallback
    /// via [`typed_char`] so the seed is the typed character, not the existing text.
    /// (Enter is NOT an edit chord — in select mode it moves down, Excel-classic.)
    BeginEdit,
    /// Escape in select mode: clear the transient degrade note (in edit mode the
    /// overlay bridge owns Escape as exact-revert, so this never fires there).
    Cancel,
    /// Clear the selected range's inputs (Delete), one undoable edit.
    Clear,
    /// Move the selected range's far corner one step, keeping the active cell (Shift+Arrow,
    /// dc-09v): the movement is the [`NavAction`]'s, applied to the corner.
    Extend(NavAction),
}

/// One row of the atlas-taggable keymap: a stable id, the chord that triggers it,
/// the action it targets, and a human label. The `id` is the durable handle a
/// future Atlas rebind surface keys on — it must never change once shipped.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct KeyBinding {
    pub id: &'static str,
    pub chord: KeyChord,
    pub action: SheetAction,
    pub label: &'static str,
}

/// The Sheet stage's whole select/navigate grammar, as one declarative table.
///
/// Ids are stable wire strings namespaced under `sheet.` so an Atlas that
/// aggregates several stages' keymaps keeps them unambiguous. F2 opens the editor
/// (edit-in-place); Enter moves the active cell DOWN in select mode (Excel-classic
/// — you edit via type-to-replace / F2 / double-click). The four Shift+Arrow
/// chords move the selected range's corner.
#[must_use]
pub fn sheet_keymap() -> Vec<KeyBinding> {
    let mut map = vec![
        // Arrow navigation (Select mode) — bare, no modifiers.
        KeyBinding {
            id: "sheet.move-up",
            chord: KeyChord::new("ArrowUp", false, false, false),
            action: SheetAction::Move(NavAction::Up),
            label: "Move up",
        },
        KeyBinding {
            id: "sheet.move-down",
            chord: KeyChord::new("ArrowDown", false, false, false),
            action: SheetAction::Move(NavAction::Down),
            label: "Move down",
        },
        KeyBinding {
            id: "sheet.move-left",
            chord: KeyChord::new("ArrowLeft", false, false, false),
            action: SheetAction::Move(NavAction::Left),
            label: "Move left",
        },
        KeyBinding {
            id: "sheet.move-right",
            chord: KeyChord::new("ArrowRight", false, false, false),
            action: SheetAction::Move(NavAction::Right),
            label: "Move right",
        },
        // Tab / Shift+Tab move the active cell one column (Excel horizontal walk).
        KeyBinding {
            id: "sheet.move-next-column",
            chord: KeyChord::new("Tab", false, false, false),
            action: SheetAction::Navigate(
                Navigation::Walk {
                    horizontal: true,
                    reverse: false,
                },
                false,
            ),
            label: "Next column",
        },
        KeyBinding {
            id: "sheet.move-prev-column",
            chord: KeyChord::new("Tab", true, false, false),
            action: SheetAction::Navigate(
                Navigation::Walk {
                    horizontal: true,
                    reverse: true,
                },
                false,
            ),
            label: "Previous column",
        },
        // Home → first column of the row; Ctrl+Home → the top-left cell.
        KeyBinding {
            id: "sheet.move-row-start",
            chord: KeyChord::new("Home", false, false, false),
            action: SheetAction::Move(NavAction::RowStart),
            label: "Row start",
        },
        KeyBinding {
            id: "sheet.move-grid-start",
            chord: KeyChord::new("Home", false, true, false),
            action: SheetAction::Move(NavAction::GridStart),
            label: "Grid start",
        },
        // F2 opens the one overlay editor at the active cell (existing text);
        // type-to-replace and double-click are the other edit entry points.
        KeyBinding {
            id: "sheet.edit-in-place",
            chord: KeyChord::new("F2", false, false, false),
            action: SheetAction::BeginEdit,
            label: "Edit in place",
        },
        // Enter in SELECT mode moves DOWN (Excel-classic) — it does not open the
        // editor. (In EDIT mode the overlay bridge owns Enter as commit+advance,
        // so this select-mode binding never fires while editing.)
        KeyBinding {
            id: "sheet.enter-down",
            chord: KeyChord::new("Enter", false, false, false),
            action: SheetAction::Navigate(
                Navigation::Walk {
                    horizontal: false,
                    reverse: false,
                },
                false,
            ),
            label: "Enter (move down)",
        },
        // Escape clears the transient degrade note in select mode.
        KeyBinding {
            id: "sheet.cancel",
            chord: KeyChord::new("Escape", false, false, false),
            action: SheetAction::Cancel,
            label: "Cancel",
        },
        // Delete clears the active cell.
        KeyBinding {
            id: "sheet.clear",
            chord: KeyChord::new("Delete", false, false, false),
            action: SheetAction::Clear,
            label: "Clear cell",
        },
        // Shift+Arrow moves the selected range's far corner.
        KeyBinding {
            id: "sheet.extend-up",
            chord: KeyChord::new("ArrowUp", true, false, false),
            action: SheetAction::Extend(NavAction::Up),
            label: "Extend selection up",
        },
        KeyBinding {
            id: "sheet.extend-down",
            chord: KeyChord::new("ArrowDown", true, false, false),
            action: SheetAction::Extend(NavAction::Down),
            label: "Extend selection down",
        },
        KeyBinding {
            id: "sheet.extend-left",
            chord: KeyChord::new("ArrowLeft", true, false, false),
            action: SheetAction::Extend(NavAction::Left),
            label: "Extend selection left",
        },
        KeyBinding {
            id: "sheet.extend-right",
            chord: KeyChord::new("ArrowRight", true, false, false),
            action: SheetAction::Extend(NavAction::Right),
            label: "Extend selection right",
        },
    ];
    for (key, nav, ctrl) in [
        ("ArrowUp", Navigation::Edge(NavAction::Up), true),
        ("ArrowDown", Navigation::Edge(NavAction::Down), true),
        ("ArrowLeft", Navigation::Edge(NavAction::Left), true),
        ("ArrowRight", Navigation::Edge(NavAction::Right), true),
        ("End", Navigation::UsedEnd, true),
        ("PageUp", Navigation::Page { down: false }, false),
        ("PageDown", Navigation::Page { down: true }, false),
    ] {
        for shift in [false, true] {
            map.push(KeyBinding {
                id: match (key, shift) {
                    ("ArrowUp", false) => "sheet.edge-up",
                    ("ArrowUp", true) => "sheet.extend-edge-up",
                    ("ArrowDown", false) => "sheet.edge-down",
                    ("ArrowDown", true) => "sheet.extend-edge-down",
                    ("ArrowLeft", false) => "sheet.edge-left",
                    ("ArrowLeft", true) => "sheet.extend-edge-left",
                    ("ArrowRight", false) => "sheet.edge-right",
                    ("ArrowRight", true) => "sheet.extend-edge-right",
                    ("End", false) => "sheet.used-end",
                    ("End", true) => "sheet.extend-used-end",
                    ("PageUp", false) => "sheet.page-up",
                    ("PageUp", true) => "sheet.extend-page-up",
                    ("PageDown", false) => "sheet.page-down",
                    _ => "sheet.extend-page-down",
                },
                chord: KeyChord::new(key, shift, ctrl, false),
                action: SheetAction::Navigate(nav, shift),
                label: "Navigate worksheet",
            });
        }
    }
    for (id, key, shift, ctrl, nav) in [
        (
            "sheet.enter-up",
            "Enter",
            true,
            false,
            Navigation::Walk {
                horizontal: false,
                reverse: true,
            },
        ),
        ("sheet.select-column", " ", false, true, Navigation::Column),
        ("sheet.select-row", " ", true, false, Navigation::Row),
        ("sheet.select-region", "a", false, true, Navigation::All),
    ] {
        map.push(KeyBinding {
            id,
            chord: KeyChord::new(key, shift, ctrl, false),
            action: SheetAction::Navigate(nav, false),
            label: "Navigate selection",
        });
    }
    for (id, key, ctrl, nav) in [
        ("sheet.extend-home", "Home", false, NavAction::RowStart),
        ("sheet.extend-grid-home", "Home", true, NavAction::GridStart),
    ] {
        map.push(KeyBinding {
            id,
            chord: KeyChord::new(key, true, ctrl, false),
            action: SheetAction::Extend(nav),
            label: "Extend to beginning",
        });
    }
    map
}

/// Resolve a chord to its action against `keymap` — a pure, first-match lookup.
/// Returns `None` for an unbound chord (the wiring then tries [`typed_char`] for
/// type-to-replace, else lets the keystroke bubble untouched to the shell).
#[must_use]
pub fn resolve_action(keymap: &[KeyBinding], chord: &KeyChord) -> Option<SheetAction> {
    keymap
        .iter()
        .find(|binding| binding.chord == *chord)
        .map(|binding| binding.action)
}

/// The single character a keydown seeds a type-to-replace edit with, or `None`
/// when the key is not a lone printable character.
///
/// A DOM `KeyboardEvent.key` is exactly one character for a printable key (`"a"`,
/// `"="`, `" "`) and a multi-character name for a control/navigation key
/// (`"Enter"`, `"ArrowUp"`, `"F2"`, `"Tab"`). This returns `Some(ch)` only for the
/// former, excluding control characters, so the wiring can seed the editor with a
/// typed character. Pure over the raw key string — the wiring is responsible for
/// having already ruled out Ctrl/Alt/Meta chords (a Ctrl+C is not type-to-replace).
#[must_use]
pub fn typed_char(key: &str) -> Option<char> {
    let mut chars = key.chars();
    match (chars.next(), chars.next()) {
        (Some(ch), None) if !ch.is_control() => Some(ch),
        _ => None,
    }
}

/// Where a keydown that reached the Sheet `<section>` goes, resolved from two
/// facts the wiring reads off the event and the stage: whether the event's
/// target is a text-entry element, and whether the stage is in EDIT. This is the
/// first step of the keydown pipeline: the SELECT grammar (arrows,
/// F2, type-to-replace) may only run when no overlay editor is open — otherwise a
/// printable key re-seeds the open editor with itself (typing `xyz` leaves `z`)
/// and Enter moves the selection instead of committing.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SectionKeyRoute {
    /// The key originated inside a text-entry element (the overlay editor's
    /// `<textarea>`): the bridge owns it — Enter = commit, Esc = revert,
    /// arrows = caret. The section does nothing.
    EditorOwns,
    /// EDIT is on but the key reached the section itself (focus fell out of the
    /// editor): hand keyboard focus back to the open editor and consume the
    /// key. The SELECT grammar must NOT run over an open editor.
    RefocusEditor,
    /// EDIT is on, the key reached the section, and it is a universal shell
    /// chord the sheet grammar does not bind (Ctrl+S, Ctrl+O, Ctrl+K, F9 —
    /// see [`is_shell_chord`]): the section leaves the event UNTOUCHED so it
    /// bubbles to the shell's keydown pipeline. Refocusing the
    /// editor here would swallow the shell's verb.
    ShellOwns,
    /// SELECT mode: the nav / edit-entry grammar resolves the key.
    SelectGrammar,
}

/// `F1`..`F24` — the shell's guard-order exemption class (mirrors
/// the shell's function-key class; a private copy so this pure
/// grammar stays Leptos-free and independently testable).
fn is_function_key(key: &str) -> bool {
    key.len() >= 2 && key.starts_with('F') && key[1..].chars().all(|ch| ch.is_ascii_digit())
}

/// A chord in the shell's universal class — Ctrl/Alt-modified (`meta` already
/// folded into `ctrl` by the wiring) or a function key. These are the chords
/// the shell's `route_key` lets through even from inside an edit buffer
/// (SHELL_SPEC §5), so a stage must never consume one it does not itself bind.
#[must_use]
pub fn is_shell_chord(chord: &KeyChord) -> bool {
    chord.ctrl || chord.alt || is_function_key(&chord.key)
}

/// Resolve a section keydown's route (see [`SectionKeyRoute`]). Pure over the
/// facts, so the wiring's initial routing is unit-tested here. While EDITING with
/// the key at the section, a shell chord the sheet grammar does not bind is
/// [`SectionKeyRoute::ShellOwns`] (Ctrl+S must save); a chord the
/// grammar binds (F2, Enter, Ctrl+Home) or plain typing refocuses the editor.
#[must_use]
pub fn section_key_route(
    target_is_text_entry: bool,
    editing: bool,
    chord: &KeyChord,
    keymap: &[KeyBinding],
) -> SectionKeyRoute {
    if target_is_text_entry {
        SectionKeyRoute::EditorOwns
    } else if editing {
        if is_shell_chord(chord) && resolve_action(keymap, chord).is_none() {
            SectionKeyRoute::ShellOwns
        } else {
            SectionKeyRoute::RefocusEditor
        }
    } else {
        SectionKeyRoute::SelectGrammar
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn key(row: u32, col: u32) -> CellKey {
        CellKey::new(calc_host::value::SheetId::from_parts(1, 0), row, col).unwrap()
    }
    fn area(top: u32, left: u32, bottom: u32, right: u32) -> GridArea {
        GridArea::new(key(top, left), key(bottom, right)).unwrap()
    }
    fn run(
        selection: &Selection,
        nav: Navigation,
        extend: bool,
        occupied: &[GridArea],
        used: Option<GridArea>,
    ) -> Selection {
        navigate(
            selection,
            nav,
            extend,
            &GridMetrics::default(),
            242.0,
            occupied,
            used,
        )
    }

    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn data_edges_distinguish_contiguous_runs_gaps_and_physical_limits() {
        let runs = [(8, 10), (1, 2), (3, 4), (20, 20)];
        for (from, forward, expected) in [
            (1, true, 4),
            (4, true, 8),
            (5, true, 8),
            (8, true, 10),
            (10, true, 20),
            (20, true, MAX_ROWS),
            (9, false, 8),
            (8, false, 4),
            (7, false, 4),
            (4, false, 1),
            (1, false, 1),
        ] {
            assert_eq!(data_edge(from, forward, MAX_ROWS, &runs), expected);
        }
        assert_eq!(data_edge(50, true, MAX_ROWS, &[]), MAX_ROWS);
        assert_eq!(
            data_edge(MAX_ROWS, true, MAX_ROWS, &[(MAX_ROWS, MAX_ROWS)]),
            MAX_ROWS
        );
    }

    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn enter_and_tab_walk_a_rectangle_keep_it_and_reverse_across_wrap() {
        let selected = area(2, 3, 4, 4);
        for (horizontal, expected) in [
            (false, vec![(3, 3), (4, 3), (2, 4), (3, 4), (4, 4), (2, 3)]),
            (true, vec![(2, 4), (3, 3), (3, 4), (4, 3), (4, 4), (2, 3)]),
        ] {
            let mut selection = Selection {
                active: Some(key(2, 3)),
                areas: vec![selected],
            };
            for (row, col) in expected {
                selection = run(
                    &selection,
                    Navigation::Walk {
                        horizontal,
                        reverse: false,
                    },
                    false,
                    &[],
                    None,
                );
                assert_eq!(selection.active, Some(key(row, col)));
                assert_eq!(selection.areas, vec![selected]);
            }
            selection = run(
                &selection,
                Navigation::Walk {
                    horizontal,
                    reverse: true,
                },
                false,
                &[],
                None,
            );
            assert_eq!(selection.active, Some(key(4, 4)));
            assert_eq!(selection.areas, vec![selected]);
        }
    }

    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn navigation_and_extension_use_sparse_spill_bounds_and_the_used_corner() {
        let selection = Selection::cell(key(2, 2));
        let spill = area(2, 2, MAX_ROWS, 2);
        let next = run(
            &selection,
            Navigation::Edge(NavAction::Down),
            false,
            &[spill],
            None,
        );
        assert_eq!(next.active, Some(key(MAX_ROWS, 2)));
        let extended = run(
            &selection,
            Navigation::Edge(NavAction::Down),
            true,
            &[spill],
            None,
        );
        assert_eq!(extended.active, selection.active);
        assert_eq!(extended.areas, vec![spill]);
        let next = run(
            &selection,
            Navigation::UsedEnd,
            false,
            &[],
            Some(area(1, 1, 500, 30)),
        );
        assert_eq!(next.active, Some(key(500, 30)));
        assert_eq!(
            run(
                &next,
                Navigation::Step(NavAction::GridStart),
                false,
                &[],
                None
            ),
            Selection::cell(key(1, 1))
        );
    }

    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn axes_and_select_all_are_rectangles_and_current_region_excludes_blank_separators() {
        let selection = Selection::cell(key(3, 2));
        let occupied = [area(1, 1, 4, 2), area(10, 1, 12, 2)];
        let region = run(&selection, Navigation::All, false, &occupied, None);
        assert_eq!(region.areas, vec![area(1, 1, 4, 2)]);
        assert_eq!(
            run(&region, Navigation::All, false, &occupied, None).areas,
            vec![area(1, 1, MAX_ROWS, MAX_COLUMNS)]
        );
        assert_eq!(
            run(&selection, Navigation::Column, false, &[], None).areas,
            vec![area(1, 2, MAX_ROWS, 2)]
        );
        assert_eq!(
            run(&selection, Navigation::Row, false, &[], None).areas,
            vec![area(3, 1, 3, MAX_COLUMNS)]
        );
        assert_eq!(
            run(
                &Selection::cell(key(8, 8)),
                Navigation::All,
                false,
                &occupied,
                None
            )
            .areas,
            vec![area(1, 1, MAX_ROWS, MAX_COLUMNS)]
        );
    }

    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn page_moves_and_hidden_axes_share_the_geometry_and_home_finds_the_first_visible_cell() {
        let mut m = GridMetrics::default();
        m.rows = super::super::geometry::Sizes::new(m.row_height, [(1, 0.0), (2, 0.0), (5, 66.0)]);
        m.cols = super::super::geometry::Sizes::new(m.col_width, [(1, 0.0), (2, 0.0)]);
        let selection = Selection::cell(key(4, 4));
        let home = navigate(
            &selection,
            Navigation::Step(NavAction::GridStart),
            false,
            &m,
            242.0,
            &[],
            None,
        );
        assert_eq!(home.active, Some(key(3, 3)));
        let page = navigate(
            &Selection::cell(key(3, 3)),
            Navigation::Page { down: true },
            false,
            &m,
            242.0,
            &[],
            None,
        );
        assert_eq!(page.active, Some(key(11, 3)));
        let previous = navigate(
            &Selection::cell(key(4, 4)),
            Navigation::Step(NavAction::Up),
            false,
            &m,
            242.0,
            &[],
            None,
        );
        assert_eq!(previous.active, Some(key(3, 4)));
        let previous = navigate(
            &previous,
            Navigation::Step(NavAction::Up),
            false,
            &m,
            242.0,
            &[],
            None,
        );
        assert_eq!(previous.active, Some(key(3, 4)));
    }

    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn subtracting_one_cell_from_a_whole_sheet_keeps_four_disjoint_rectangles() {
        let whole = area(1, 1, MAX_ROWS, MAX_COLUMNS);
        let parts = subtract(whole, area(10, 10, 10, 10));
        assert_eq!(parts.len(), 4);
        assert_eq!(
            parts.iter().map(|a| a.cell_count()).sum::<u64>(),
            whole.cell_count() - 1
        );
        assert!(parts.iter().all(|a| !a.contains(key(10, 10))));
        for (i, a) in parts.iter().enumerate() {
            for b in parts.iter().skip(i + 1) {
                assert!(
                    a.last().row() < b.first().row()
                        || b.last().row() < a.first().row()
                        || a.last().column() < b.first().column()
                        || b.last().column() < a.first().column()
                );
            }
        }
    }

    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn disjoint_rectangles_walk_in_both_directions_without_expanding_cells() {
        let selection = Selection {
            active: Some(key(2, 2)),
            areas: vec![
                area(2, 2, 2, 2),
                area(MAX_ROWS, MAX_COLUMNS, MAX_ROWS, MAX_COLUMNS),
            ],
        };
        let next = run(
            &selection,
            Navigation::Walk {
                horizontal: false,
                reverse: false,
            },
            false,
            &[],
            None,
        );
        assert_eq!(next.active, Some(key(MAX_ROWS, MAX_COLUMNS)));
        assert_eq!(next.areas, selection.areas);
        assert_eq!(
            run(
                &next,
                Navigation::Walk {
                    horizontal: false,
                    reverse: true
                },
                false,
                &[],
                None
            ),
            selection
        );
    }

    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn extending_mixed_corners_keeps_the_stationary_anchor_and_both_axes() {
        let selected = area(1, 1, 3, 2);
        let upper_right = Selection {
            active: Some(key(1, 2)),
            areas: vec![selected],
        };
        let next = run(
            &upper_right,
            Navigation::Step(NavAction::Down),
            true,
            &[],
            None,
        );
        assert_eq!(next.active, upper_right.active);
        assert_eq!(next.areas, vec![area(1, 1, 4, 2)]);
        let lower_left = Selection {
            active: Some(key(3, 1)),
            areas: vec![selected],
        };
        let next = run(
            &lower_left,
            Navigation::Step(NavAction::Right),
            true,
            &[],
            None,
        );
        assert_eq!(next.active, lower_left.active);
        assert_eq!(next.areas, vec![area(1, 1, 3, 3)]);
    }

    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn range_wrap_and_disjoint_wrap_skip_hidden_boundary_bands() {
        let mut metrics = GridMetrics::default();
        metrics.rows =
            super::super::geometry::Sizes::new(metrics.row_height, [(1, 0.0), (10, 0.0)]);
        let selection = Selection {
            active: Some(key(3, 1)),
            areas: vec![area(1, 1, 3, 2)],
        };
        let next = navigate(
            &selection,
            Navigation::Walk {
                horizontal: false,
                reverse: false,
            },
            false,
            &metrics,
            242.0,
            &[],
            None,
        );
        assert_eq!(next.active, Some(key(2, 2)));
        assert_eq!(next.areas, selection.areas);
        let selection = Selection {
            active: Some(key(3, 2)),
            areas: vec![area(1, 1, 3, 2), area(10, 3, 12, 4)],
        };
        let next = navigate(
            &selection,
            Navigation::Walk {
                horizontal: false,
                reverse: false,
            },
            false,
            &metrics,
            242.0,
            &[],
            None,
        );
        assert_eq!(next.active, Some(key(11, 3)));
        assert_eq!(next.areas, selection.areas);
    }

    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn axis_selection_expands_every_row_or_column_of_one_rectangle() {
        let selection = Selection {
            active: Some(key(2, 2)),
            areas: vec![area(2, 2, 3, 3)],
        };
        assert_eq!(
            run(&selection, Navigation::Column, false, &[], None).areas,
            vec![area(1, 2, MAX_ROWS, 3)]
        );
        assert_eq!(
            run(&selection, Navigation::Row, false, &[], None).areas,
            vec![area(2, 1, 3, MAX_COLUMNS)]
        );
    }

    const DEMO: GridExtent = GridExtent { rows: 5, cols: 2 };

    /// The keydown pipeline's first step. A key from inside the
    /// editor's textarea is the bridge's; a key that reaches the section WHILE
    /// EDITING must route focus back into the editor rather than run the SELECT
    /// grammar (the re-seed / Enter-moves bug); only SELECT mode resolves keys.
    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn section_key_route_never_runs_the_select_grammar_over_an_open_editor() {
        let keymap = sheet_keymap();
        let typed = KeyChord::new("x", false, false, false);
        let enter = KeyChord::new("Enter", false, false, false);
        assert_eq!(
            section_key_route(true, true, &typed, &keymap),
            SectionKeyRoute::EditorOwns
        );
        assert_eq!(
            section_key_route(true, false, &typed, &keymap),
            SectionKeyRoute::EditorOwns
        );
        assert_eq!(
            section_key_route(false, true, &typed, &keymap),
            SectionKeyRoute::RefocusEditor,
            "EDIT on + a key at the section = refocus the editor, never re-seed it"
        );
        assert_eq!(
            section_key_route(false, true, &enter, &keymap),
            SectionKeyRoute::RefocusEditor,
            "Enter at the section while EDITING refocuses the editor (it commits there)"
        );
        assert_eq!(
            section_key_route(false, false, &typed, &keymap),
            SectionKeyRoute::SelectGrammar
        );
    }

    /// Ctrl+S must still save while focus is inside the grid.
    /// EDIT on with the key at the section (focus fell out of the editor: a
    /// header click, a focus race) used to REFOCUS the editor and consume every
    /// key, so the shell's universal chords died here. A shell chord the sheet
    /// grammar does not bind must be left untouched (`ShellOwns`) so it bubbles
    /// to the shell's Save / Open / deck / Recalculate verbs; chords the grammar
    /// DOES bind (F2, Ctrl+Home) keep refocusing the editor.
    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn section_lets_unbound_shell_chords_bubble_while_editing() {
        let keymap = sheet_keymap();
        let ctrl_s = KeyChord::new("s", false, true, false);
        let ctrl_o = KeyChord::new("o", false, true, false);
        let ctrl_k = KeyChord::new("k", false, true, false);
        let f9 = KeyChord::new("F9", false, false, false);
        for chord in [&ctrl_s, &ctrl_o, &ctrl_k, &f9] {
            assert!(is_shell_chord(chord), "{chord:?} is a shell chord");
            assert_eq!(
                section_key_route(false, true, chord, &keymap),
                SectionKeyRoute::ShellOwns,
                "{chord:?} at the section while EDITING must bubble to the shell"
            );
            // From inside the editor's textarea the bridge owns propagation.
            assert_eq!(
                section_key_route(true, true, chord, &keymap),
                SectionKeyRoute::EditorOwns
            );
            // In SELECT mode the grammar runs (and leaves an unbound chord
            // untouched so it bubbles — the wiring's existing fall-through).
            assert_eq!(
                section_key_route(false, false, chord, &keymap),
                SectionKeyRoute::SelectGrammar
            );
        }
        // Chords the sheet grammar binds still hand focus back to the editor.
        let f2 = KeyChord::new("F2", false, false, false);
        let ctrl_home = KeyChord::new("Home", false, true, false);
        for chord in [&f2, &ctrl_home] {
            assert_eq!(
                section_key_route(false, true, chord, &keymap),
                SectionKeyRoute::RefocusEditor,
                "{chord:?} is the sheet's own — refocus the editor"
            );
        }
        // Plain typing is not a shell chord.
        assert!(!is_shell_chord(&KeyChord::new("s", false, false, false)));
        assert!(!is_shell_chord(&KeyChord::new(
            "Enter", false, false, false
        )));
    }

    /// The four arrow moves step the active cell by one in the expected axis,
    /// interior to the extent (no clamping in play here).
    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn arrows_step_one_cell_in_the_interior() {
        let start = (3, 1);
        assert_eq!(next_active(start, NavAction::Up, DEMO), (2, 1));
        assert_eq!(next_active(start, NavAction::Down, DEMO), (4, 1));
        assert_eq!(next_active((3, 2), NavAction::Left, DEMO), (3, 1));
        assert_eq!(next_active(start, NavAction::Right, DEMO), (3, 2));
    }

    /// Tab / Shift+Tab walk columns; Home / Ctrl+Home jump to the row start and
    /// the grid origin.
    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn tab_home_and_grid_start_land_where_expected() {
        assert_eq!(next_active((3, 1), NavAction::NextColumn, DEMO), (3, 2));
        assert_eq!(next_active((3, 2), NavAction::PrevColumn, DEMO), (3, 1));
        assert_eq!(next_active((4, 2), NavAction::RowStart, DEMO), (4, 1));
        assert_eq!(next_active((4, 2), NavAction::GridStart, DEMO), (1, 1));
    }

    /// An accepted Enter-commit advances one row down, clamping at the last row.
    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn commit_down_advances_and_clamps_at_the_last_row() {
        assert_eq!(next_active((2, 2), NavAction::CommitDown, DEMO), (3, 2));
        // Already on the last row: stays put (no wrap to row 1).
        assert_eq!(next_active((5, 2), NavAction::CommitDown, DEMO), (5, 2));
    }

    /// An accepted Tab-commit advances one column right, clamping at the last
    /// column (the horizontal twin of the Enter-commit).
    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn commit_right_advances_and_clamps_at_the_last_column() {
        assert_eq!(next_active((2, 1), NavAction::CommitRight, DEMO), (2, 2));
        // Already on the last column: stays put (no wrap to column 1).
        assert_eq!(next_active((2, 2), NavAction::CommitRight, DEMO), (2, 2));
    }

    /// THE clamping invariant: a move off every edge parks on the edge cell — the
    /// active cell can never leave `[1, rows] × [1, cols]`, and never wraps.
    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn moves_clamp_at_every_edge_with_no_wrap() {
        // Top edge: Up from row 1 stays on row 1.
        assert_eq!(next_active((1, 1), NavAction::Up, DEMO), (1, 1));
        // Left edge: Left / Shift+Tab from col 1 stay on col 1.
        assert_eq!(next_active((3, 1), NavAction::Left, DEMO), (3, 1));
        assert_eq!(next_active((3, 1), NavAction::PrevColumn, DEMO), (3, 1));
        // Bottom edge: Down from the last row stays on the last row.
        assert_eq!(next_active((5, 1), NavAction::Down, DEMO), (5, 1));
        // Right edge: Right / Tab from the last column stay on the last column.
        assert_eq!(next_active((3, 2), NavAction::Right, DEMO), (3, 2));
        assert_eq!(next_active((3, 2), NavAction::NextColumn, DEMO), (3, 2));
    }

    /// A degenerate `(0, 0)` extent floors to a single legal cell, so navigation
    /// over an empty grid can never produce an out-of-range address.
    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn degenerate_extent_floors_to_one_cell() {
        let empty = GridExtent::new(0, 0);
        assert_eq!(empty, GridExtent { rows: 1, cols: 1 });
        assert_eq!(next_active((1, 1), NavAction::Down, empty), (1, 1));
        assert_eq!(next_active((1, 1), NavAction::Right, empty), (1, 1));
    }

    /// A starting cell already outside the extent (e.g. a stale active cell after
    /// the grid shrank) is pulled back inside by the clamp on the very next move.
    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn out_of_extent_start_is_pulled_back_inside() {
        assert_eq!(next_active((99, 99), NavAction::Up, DEMO), (5, 2));
        assert_eq!(next_active((99, 99), NavAction::RowStart, DEMO), (5, 1));
    }

    /// Every declared chord resolves to its declared action — the grammar's
    /// chord→action mapping asserted against the live table.
    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn every_declared_chord_resolves_to_its_action() {
        let keymap = sheet_keymap();
        for (chord, expected) in [
            (
                KeyChord::new("ArrowUp", false, false, false),
                SheetAction::Move(NavAction::Up),
            ),
            (
                KeyChord::new("ArrowDown", false, false, false),
                SheetAction::Move(NavAction::Down),
            ),
            (
                KeyChord::new("ArrowLeft", false, false, false),
                SheetAction::Move(NavAction::Left),
            ),
            (
                KeyChord::new("ArrowRight", false, false, false),
                SheetAction::Move(NavAction::Right),
            ),
            (
                KeyChord::new("Tab", false, false, false),
                SheetAction::Navigate(
                    Navigation::Walk {
                        horizontal: true,
                        reverse: false,
                    },
                    false,
                ),
            ),
            (
                KeyChord::new("Tab", true, false, false),
                SheetAction::Navigate(
                    Navigation::Walk {
                        horizontal: true,
                        reverse: true,
                    },
                    false,
                ),
            ),
            (
                KeyChord::new("Home", false, false, false),
                SheetAction::Move(NavAction::RowStart),
            ),
            (
                KeyChord::new("Home", false, true, false),
                SheetAction::Move(NavAction::GridStart),
            ),
            (
                KeyChord::new("F2", false, false, false),
                SheetAction::BeginEdit,
            ),
            (
                // Enter walks a selected rectangle, or moves down from one cell.
                KeyChord::new("Enter", false, false, false),
                SheetAction::Navigate(
                    Navigation::Walk {
                        horizontal: false,
                        reverse: false,
                    },
                    false,
                ),
            ),
            (
                KeyChord::new("Escape", false, false, false),
                SheetAction::Cancel,
            ),
            (
                KeyChord::new("ArrowUp", true, false, false),
                SheetAction::Extend(NavAction::Up),
            ),
        ] {
            assert_eq!(
                resolve_action(&keymap, &chord),
                Some(expected),
                "chord {chord:?} must resolve to {expected:?}"
            );
        }
    }

    /// A chord not in the table resolves to `None` — the wiring must let it fall
    /// through to type-to-replace or bubble, never swallow it. Ctrl+arrow (the
    /// edge-jump chord, not built) is deliberately
    /// unbound here, so it bubbles rather than silently window-jumping.
    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn non_matching_chords_resolve_to_none() {
        let keymap = sheet_keymap();
        assert_eq!(
            resolve_action(&keymap, &KeyChord::new("ArrowUp", false, true, false)),
            Some(SheetAction::Navigate(
                Navigation::Edge(NavAction::Up),
                false
            )),
            "Ctrl+ArrowUp follows the data edge"
        );
        // A bare letter is not a nav chord — it falls through to type-to-replace.
        assert_eq!(
            resolve_action(&keymap, &KeyChord::new("a", false, false, false)),
            None
        );
        // Shift+Enter walks the rectangle in reverse.
        assert_eq!(
            resolve_action(&keymap, &KeyChord::new("Enter", true, false, false)),
            Some(SheetAction::Navigate(
                Navigation::Walk {
                    horizontal: false,
                    reverse: true
                },
                false
            ))
        );
    }

    /// Delete clears; Shift+Arrow moves the range's corner in its direction.
    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn delete_clears_and_shift_arrows_extend() {
        let keymap = sheet_keymap();
        assert_eq!(
            resolve_action(&keymap, &KeyChord::new("Delete", false, false, false)),
            Some(SheetAction::Clear)
        );
        assert_eq!(
            resolve_action(&keymap, &KeyChord::new("ArrowRight", true, false, false)),
            Some(SheetAction::Extend(NavAction::Right))
        );
        assert_eq!(
            resolve_action(&keymap, &KeyChord::new("ArrowDown", true, false, false)),
            Some(SheetAction::Extend(NavAction::Down))
        );
    }

    /// The table is well-formed: every binding id is unique and stable, and F2 is
    /// the single chord that opens the editor (Enter moves down in select mode,
    /// Excel-classic; type-to-replace and double-click are the other edit entries).
    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn keymap_ids_are_unique_and_f2_is_the_begin_edit_chord() {
        let keymap = sheet_keymap();
        let mut ids: Vec<&str> = keymap.iter().map(|b| b.id).collect();
        let total = ids.len();
        ids.sort_unstable();
        ids.dedup();
        assert_eq!(ids.len(), total, "every binding id must be unique");

        let begin_edit = keymap
            .iter()
            .filter(|b| b.action == SheetAction::BeginEdit)
            .count();
        assert_eq!(begin_edit, 1, "F2 is the only begin-edit chord");
    }

    /// The uppercase (shifted) form of an alphabetic key normalizes to the same
    /// chord key as its lowercase form, so `Shift` is carried by the flag, never
    /// by the case — the property type-to-replace relies on for letters.
    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn uppercase_alpha_normalizes_to_lowercase_key() {
        assert_eq!(KeyChord::new("A", true, false, false).key, "a");
        assert_eq!(KeyChord::new("ArrowUp", false, false, false).key, "ArrowUp");
    }

    /// `typed_char` recognizes exactly the lone printable keys (letters, digits,
    /// punctuation, space) and rejects every multi-character control/navigation
    /// key name and the empty string.
    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn typed_char_recognizes_only_lone_printables() {
        for (key, expected) in [
            ("a", Some('a')),
            ("A", Some('A')),
            ("=", Some('=')),
            ("7", Some('7')),
            (" ", Some(' ')),
        ] {
            assert_eq!(typed_char(key), expected, "key {key:?}");
        }
        for control in [
            "Enter", "ArrowUp", "F2", "Tab", "Home", "Escape", "", "\u{1b}",
        ] {
            assert_eq!(
                typed_char(control),
                None,
                "key {control:?} is not printable"
            );
        }
    }
}
