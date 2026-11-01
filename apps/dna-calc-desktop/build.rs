fn main() {
    println!("cargo:rerun-if-changed=version.rc");
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() != Ok("windows") {
        return;
    }
    println!("cargo:rustc-link-arg-bin=dna-calc-desktop=/EXPORT:MdCallBack12");
    let kits =
        std::path::PathBuf::from(std::env::var_os("ProgramFiles(x86)").expect("ProgramFiles(x86)"))
            .join("Windows Kits/10/bin");
    let mut versions = std::fs::read_dir(&kits)
        .expect("Windows SDK bin directory")
        .filter_map(Result::ok)
        .map(|e| e.path())
        .filter(|p| p.join("x64/rc.exe").is_file())
        .collect::<Vec<_>>();
    versions.sort();
    let rc = versions
        .last()
        .expect("Windows SDK resource compiler")
        .join("x64/rc.exe");
    let output = std::path::PathBuf::from(std::env::var_os("OUT_DIR").unwrap()).join("version.res");
    let status = std::process::Command::new(rc)
        .arg("/nologo")
        .arg("/fo")
        .arg(&output)
        .arg("version.rc")
        .status()
        .expect("run rc.exe");
    assert!(
        status.success(),
        "compile native executable version resource"
    );
    println!(
        "cargo:rustc-link-arg-bin=dna-calc-desktop={}",
        output.display()
    );
}
