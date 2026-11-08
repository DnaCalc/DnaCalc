//! The loaded add-in set of a desktop application, once.
//!
//! OneCalc and DNA Calc both load add-ins, and both used to assemble the same pieces by hand: the
//! native XLL host, the Excel object model those XLLs see (`calc-com-host`'s projection, answering
//! `WM_GETOBJECT` on the shell's window chain), the RTD client, the VBA modules, and the one
//! callable table that publishes every loaded function to CalcFml. [`AddinHost`] owns them and
//! nothing else does.
//!
//! What this crate does not know: any workbook, engine, document codec or user interface. It
//! reaches CalcFml (`CallableTable`, the user-function contract), CalcFunc's values, `calc-vba`
//! and the two native hosts, so it sits below OneCalc's dependency ceiling
//! (`tools/xtask/architecture.json` forbids `calc-host`, `calc-engine` and `calc-doc` here). An
//! application hands the table to its own owner of formulas (OneCalc's `Session`, DNA Calc's
//! workbook worker); what happens to a sheet cell stays that owner's business, and
//! [`AddinHost::sheet_changed`] and [`AddinHost::after_calculate`] only tell add-ins it happened.
//!
//! The XLL and COM halves of this crate need the Windows x64 shell (`calc-platform-desktop`), and
//! nothing here compiles on other platforms.
#[cfg(target_arch = "wasm32")]
compile_error!("calc-addin-host is native-only");

#[cfg(windows)]
mod host;
#[cfg(windows)]
pub use host::{AddinHost, Document, VbaAddinLoad};

#[cfg(windows)]
pub use calc_com_host::{Projection, RtdClient, RtdInput};
#[cfg(windows)]
pub use calc_fml::CallableTable;
#[cfg(windows)]
pub use calc_xll_host::{AsyncReturn, RegistrationInfo, RtdProvider, XllHost};

/// Must run before this process creates a thread or loads an add-in. Older Excel-DNA releases
/// run their Excel-only start-up when the process is called `EXCEL.EXE`; this opts a host with
/// its own name in. (Older releases can ignore it; the executable keeps its own name.)
#[cfg(windows)]
pub fn prepare_process() {
    // SAFETY: the caller runs this single-threaded, at the start of `main`.
    unsafe { std::env::set_var("EXCELDNA_ASSUME_EXCEL_PROCESS", "1") };
}

/// A value as the harnesses' JSON reports show it: numbers, logicals and text as themselves,
/// an error as `{"error": "#N/A"}`, a blank as null, anything else as its debug text.
#[cfg(windows)]
pub fn value_json(value: &calc_value::CalcValue) -> serde_json::Value {
    use calc_value::CoreRef;
    match value.core_ref() {
        CoreRef::Number(n) => serde_json::json!(n),
        CoreRef::Logical(b) => serde_json::json!(b),
        CoreRef::Text(t) => serde_json::json!(String::from_utf16_lossy(t.utf16_code_units())),
        CoreRef::Error(e) => serde_json::json!({"error":e.excel_spelling()}),
        CoreRef::Empty => serde_json::Value::Null,
        other => serde_json::json!({"value":format!("{other:?}")}),
    }
}
