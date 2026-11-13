#[path = "../calc-object-model/src/descriptor.rs"]
#[allow(dead_code)]
mod descriptor;
mod generate;

fn main() {
    generate::write();
}
