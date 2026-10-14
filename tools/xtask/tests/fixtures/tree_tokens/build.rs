// Fixture build script for check (d): its code is in scope, its plain comments (tree reference)
// are not.

fn main() {
    println!("cargo::rustc-env=TREE_REF_COUNT=0");
    let _ = include_bytes!("build.rs");
}
