//! Export catalogue call metadata for the seeded witness-bank generator.
//! Run: cargo run -q -p calc-catalogue --example export_witness_schema > catalogue.tsv
extern crate calc_catalogue;
fn main() {
    for d in calc_catalogue::declarations() {
        if d.notation() != calc_catalogue::Notation::Call {
            continue;
        }
        let a = d.arity();
        print!(
            "{}\t{:?}\t{:?}\t{}\t{}\t{:?}\t",
            d.spelling(),
            d.kind(),
            d.implementation(),
            a.min,
            a.max,
            d.groups()
        );
        for p in d.parameters() {
            print!("{}:{:?}:{:?},", p.name, p.class, p.entry);
        }
        println!();
    }
}
