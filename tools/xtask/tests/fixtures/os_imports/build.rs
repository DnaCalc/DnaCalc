// Fixture build script for check (b): build scripts run on the build host, so the std::fs below is
// not reported, but an include macro is.

fn main() {
    let _ = std::fs::read("build.rs");
    let _ = include_str!("build.rs");
}
