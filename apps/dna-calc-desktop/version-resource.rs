// Shared by the two desktop executables' build scripts (`include!`d, never built alone): compiles
// the crate's `version.rc` and links it, and the MdCallBack12 export, into the binary `bin`.
// Add-ins read the version resource (16.x) to see an Excel-like host, and call MdCallBack12 by name.
fn link_executable_identity(bin: &str) {
    println!("cargo:rerun-if-changed=version.rc");
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() != Ok("windows") {
        return;
    }
    println!("cargo:rustc-link-arg-bin={bin}=/EXPORT:MdCallBack12");
    // A Windows build host uses the SDK resource compiler; a cross build (cargo xwin on Linux,
    // docs/plan/WINDOWS_CROSS_BUILD.md) uses llvm-rc, which reads this self-contained .rc as is.
    let rc = if cfg!(windows) {
        let kits = std::path::PathBuf::from(
            std::env::var_os("ProgramFiles(x86)").expect("ProgramFiles(x86)"),
        )
        .join("Windows Kits/10/bin");
        let mut versions = std::fs::read_dir(&kits)
            .expect("Windows SDK bin directory")
            .filter_map(Result::ok)
            .map(|e| e.path())
            .filter(|p| p.join("x64/rc.exe").is_file())
            .collect::<Vec<_>>();
        versions.sort();
        versions
            .last()
            .expect("Windows SDK resource compiler")
            .join("x64/rc.exe")
    } else {
        std::path::PathBuf::from(std::env::var_os("LLVM_RC").unwrap_or_else(|| "llvm-rc".into()))
    };
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
    println!("cargo:rustc-link-arg-bin={bin}={}", output.display());
}
