// Validate the committed overlays with the same loader used by the projections.
#[allow(dead_code)]
#[path = "src/descriptor.rs"]
mod descriptor;

fn main() {
    println!("cargo:rerun-if-changed=descriptor");
    println!("cargo:rerun-if-changed=src/descriptor.rs");
    descriptor::Descriptor::get();
}
