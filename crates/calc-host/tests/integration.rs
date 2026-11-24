//! Integration suites for this package.
//! xtask accept selects named cases under acceptance_cases.

#[path = "acceptance.rs"]
mod acceptance_cases;

#[path = "automation.rs"]
mod automation;

#[path = "charts.rs"]
mod charts;

#[path = "charts_ui28.rs"]
mod charts_ui28;

#[path = "circular.rs"]
mod circular;

#[path = "circular_game.rs"]
mod circular_game;

#[path = "conditional.rs"]
mod conditional;

#[path = "cube.rs"]
mod cube;

#[path = "dc_1nbp_12_edit_time_refusals.rs"]
mod dc_1nbp_12_edit_time_refusals;

#[path = "euroconvert.rs"]
mod euroconvert;

#[path = "events.rs"]
mod events;

#[path = "external_links.rs"]
mod external_links;

#[path = "fetch.rs"]
mod fetch;

#[path = "held_arrays.rs"]
mod held_arrays;

#[path = "import.rs"]
mod import;

#[path = "incremental.rs"]
mod incremental;

#[path = "names.rs"]
mod names;

#[path = "pivot_m5.rs"]
mod pivot_m5;

#[path = "pivot_ui28.rs"]
mod pivot_ui28;

#[path = "pivot_ui28_refresh.rs"]
mod pivot_ui28_refresh;

#[path = "print_members.rs"]
mod print_members;

#[path = "reference_arrays.rs"]
mod reference_arrays;

#[path = "report_objects.rs"]
mod report_objects;

#[path = "round_trip.rs"]
mod round_trip;

#[path = "threads.rs"]
mod threads;

#[path = "validation.rs"]
mod validation;
