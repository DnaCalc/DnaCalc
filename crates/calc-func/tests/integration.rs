//! Integration suites for this package.
//! xtask accept selects named cases under acceptance_cases.

#[path = "acceptance.rs"]
mod acceptance_cases;

#[path = "f1_engineering.rs"]
mod f1_engineering;

#[path = "f1_math.rs"]
mod f1_math;

#[path = "f1_stats.rs"]
mod f1_stats;

#[path = "f2_dates.rs"]
mod f2_dates;

#[path = "f2_text.rs"]
mod f2_text;

#[path = "f3_arrays.rs"]
mod f3_arrays;

#[path = "f3_financial.rs"]
mod f3_financial;

#[path = "import_excel.rs"]
mod import_excel;

#[path = "jnt_inputs.rs"]
mod jnt_inputs;

#[path = "witnesses.rs"]
mod witnesses;

#[path = "f1_support/x87_cpu.rs"]
mod x87_cpu;
