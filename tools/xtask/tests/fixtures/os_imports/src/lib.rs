// Fixture for check (b): every denied import below is found with its line, and the test-only
// module, the string and the doc comment are not.

/// Mentions std::fs in a doc comment, which is not an import.
pub fn doc_only() {}

pub const TEXT: &str = "std::process::Command in a string is not an import";

pub fn read(path: &str) -> Vec<u8> {
    std::fs::read(path).unwrap_or_default()
}
use std::{io, net};

use std::*;

extern crate std as standard;

pub fn id() -> String {
    format!("{}", std::process::id())
}

#[link(name = "kernel32")]
unsafe extern "system" {
    fn GetTickCount() -> u32;
}

pub fn raw() -> usize {
    std::mem::size_of::<std::os::raw::c_int>()
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::process::Command;
}

pub const BYTES: &[u8] = include_bytes!("lib.rs");
