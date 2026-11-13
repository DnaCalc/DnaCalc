//! com-descriptor: reads a COM type library and writes a JSON descriptor of selected types.
//!
//! The descriptor is the source of Excel's object-model interface facts (IIDs, DISPIDs, vtable
//! slots and exact parameter lists) from which DnaCalc generates its Rust COM projection. It
//! carries names and ABI facts only: no documentation strings, help contexts or help files.
//!
//! ```text
//! com-descriptor (--library <LIBID>,<major.minor> | --file <path>) --interfaces <list> --out <json>
//! com-descriptor (--library ... | --file ...) --list
//! ```
//!
//! `--library` loads the registered library (LoadRegTypeLib, LCID 0); `--file` loads a file with
//! LoadTypeLibEx(REGKIND_NONE). Nothing is ever registered. The interface list is plain text, one
//! type name per line, `#` starting a comment. A listed name the library does not define is
//! reported on stderr and the run exits 1 after writing the descriptor of the names it found.
//!
//! Native only, and Windows only: elsewhere it prints that it needs Windows and exits 2.

#[cfg(target_arch = "wasm32")]
compile_error!("com-descriptor is native-only and must not be built for wasm32");

#[cfg(windows)]
mod typelib;

#[cfg(windows)]
fn main() {
    std::process::exit(typelib::run(std::env::args().skip(1).collect()));
}

#[cfg(not(windows))]
fn main() {
    eprintln!("com-descriptor needs Windows: it reads COM type libraries through oleaut32");
    std::process::exit(2);
}
