//! `dnacalc`: see `dnacalc --help` and `lib.rs`.
fn main() -> std::process::ExitCode {
    // The VBA runtime and the evaluator recurse deeply; the main thread's default stack is too
    // small for them, as for the corpus tests.
    let child = std::thread::Builder::new()
        .stack_size(256 * 1024 * 1024)
        .spawn(|| {
            let args: Vec<String> = std::env::args().skip(1).collect();
            dnacalc::run(&args, &mut std::io::stdout(), &mut std::io::stderr())
        })
        .expect("the worker thread starts");
    match child.join() {
        Ok(code) => std::process::ExitCode::from(code),
        Err(_) => std::process::ExitCode::from(70),
    }
}
