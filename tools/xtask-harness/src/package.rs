//! `cargo xtask package onecalc [--out <dir>] [--no-zip] [--without-calc]`: DNA OneCalc as one folder that
//! runs on a clean Windows machine (milestone 7, dc-0f1k.13), and a zip of it.
//!
//! The command
//! 1. builds OneCalc's page (`cargo xtask app bundle onecalc`) and packs its files into one block
//!    (`calc_platform_desktop::assets`); DNA Calc's page likewise (`cargo xtask app bundle calc`), unless
//!    `--without-calc`;
//! 2. builds `dna-onecalc-desktop`, `dna-calc-desktop` (the optional companion OneCalc's "Open in DNA
//!    Calc" starts) and the `XLCALL32.DLL` companion in the release profile with the C runtime linked
//!    statically (`-C target-feature=+crt-static`, so the folder needs no Visual C++ redistributable) and
//!    each page embedded in its executable (`DNACALC_ONECALC_PAGE`, `DNACALC_CALC_PAGE`, read by the
//!    executables' `build.rs`, which also make them window programs);
//! 3. checks the executables' import tables: they must name no C++ runtime DLL;
//! 4. assembles `<out>/DNA-OneCalc/` (the executables, `XLCALL32.DLL`, `README.txt`, `LICENSE`), writes
//!    `<out>/DNA-OneCalc-<version>-win-x64.zip` of it, and records every file's size and SHA-256, the
//!    zip's, the page's files, the executable's imports and the commit in
//!    `<out>/package-manifest.json`.
//!
//! `cargo xtask package onecalc-clean-run ...` is the proof that the folder works on a clean
//! machine (`package_clean.rs`).

use crate::app::{APPS, bundle};
use crate::cargo::{self, Workspace};
use calc_platform_desktop::assets;
use serde_json::{Value, json};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, ExitCode};

mod installer;
const WINDOWS_TARGET: &str = "x86_64-pc-windows-msvc";

pub const USAGE: &str = "usage: cargo xtask package onecalc [--out <dir>] [--no-zip] [--without-calc] [--target x86_64-pc-windows-msvc]\n       cargo xtask package installer [--package <existing package dir>] [--out <dir>] [--wixl <path>]\n       cargo xtask package onecalc-clean-run --package <DNA-OneCalc folder> --work <empty dir> --out <dir> [--fixtures <dir>] [--x5 <X5Native64.xll>]";

/// The folder the zip holds, and the executable's name in it.
pub const FOLDER: &str = "DNA-OneCalc";
pub const EXE: &str = "dna-onecalc-desktop.exe";
/// The optional companion: DNA Calc, which OneCalc starts for a file it cannot open as a session.
pub const CALC_EXE: &str = "dna-calc-desktop.exe";
/// DNA Calc's own folder (`--without-calc` leaves it out): the same executable on its own, with its README.
pub const CALC_FOLDER: &str = "DNA-Calc";

const CALC_README: &str = "DNA Calc\r\n\r\nThe Excel workbook application on your desktop. Run dna-calc-desktop.exe.\r\n\r\nWhat you can do:\r\n  - open an Excel workbook: start dna-calc-desktop.exe FILE (or --open FILE) and its sheets,\r\n    cells, formulas, formats and charts show in the window;\r\n  - edit cells and formulas, which recalculate through the same function library as DNA OneCalc;\r\n  - export and print from the page (downloads go through WebView2's own save prompt);\r\n  - load Excel add-ins from the command line: dna-calc-desktop.exe --xll PATH.\r\n\r\nThis folder is the whole program:\r\n  dna-calc-desktop.exe  the application; its page is inside it\r\n  XLCALL32.DLL          lets Excel add-ins (.xll) find the application, as they find Excel\r\n  LICENSE               MIT licence\r\n\r\nIt needs the Microsoft Edge WebView2 Runtime (the Evergreen runtime, already part of Windows 11 and\r\nof current Microsoft Edge installations). If it is missing the application says so and gives the\r\ndownload page: https://developer.microsoft.com/microsoft-edge/webview2/\r\n\r\ndna-calc-desktop.exe --version prints the version. The browser profile is kept under\r\n%LOCALAPPDATA%\\DNA Calc\\webview2. Delete that folder to forget it.\r\n";

const README: &str = "DNA OneCalc\r\n\r\nOne Excel formula at a time, with the inputs it needs, on your desktop. Run dna-onecalc-desktop.exe.\r\n\r\nWhat you can do:\r\n  - type an Excel formula, see its result, its parts and a step-by-step trace; format the result;\r\n  - open and save a session file (Ctrl+O, Ctrl+S, Ctrl+Shift+S; recent files are remembered);\r\n  - load Excel add-ins (.xll, .xlam) and use their functions; open an .xlsm session with its macros;\r\n  - the window title shows the session name, with * for unsaved changes, and asks before closing it;\r\n  - dna-onecalc-desktop.exe --version prints the version.\r\n\r\nThis folder is the whole program:\r\n  dna-onecalc-desktop.exe  the application; its page is inside it\r\n  dna-calc-desktop.exe     DNA Calc, the workbook application; its page is inside it\r\n  XLCALL32.DLL             lets Excel add-ins (.xll) find the application, as they find Excel\r\n  LICENSE                  MIT licence\r\n\r\nIt needs the Microsoft Edge WebView2 Runtime (the Evergreen runtime, already part of Windows 11 and\r\nof current Microsoft Edge installations). If it is missing the application says so and gives the\r\ndownload page: https://developer.microsoft.com/microsoft-edge/webview2/\r\n\r\nExcel add-ins (.xll, .xlam) load from the Add-ins panel. A file OneCalc cannot open as a session can\r\nbe opened in DNA Calc when dna-calc-desktop.exe is in this folder.\r\n\r\nThe application keeps its settings (window place, recent files) and its browser profile under\r\n%LOCALAPPDATA%\\DNA OneCalc. Delete that folder to forget them.\r\n";

pub fn run(raw: &[String]) -> ExitCode {
    match raw.split_first() {
        Some((command, rest)) if command == "onecalc" => build(rest),
        Some((command, rest)) if command == "installer" => installer::run(rest),
        #[cfg(windows)]
        Some((command, rest)) if command == "onecalc-clean-run" => crate::app::clean::run(rest),
        #[cfg(not(windows))]
        Some((command, _)) if command == "onecalc-clean-run" => {
            eprintln!("the clean run drives the Windows desktop application");
            ExitCode::from(2)
        }
        _ => {
            eprintln!("{USAGE}");
            ExitCode::from(2)
        }
    }
}

fn build(raw: &[String]) -> ExitCode {
    let mut out = None;
    let mut zip = true;
    let mut calc = true;
    let mut args = raw.iter();
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--out" => match args.next() {
                Some(path) => out = Some(PathBuf::from(path)),
                None => return ExitCode::from(2),
            },
            "--target" if args.next().map(String::as_str) == Some(WINDOWS_TARGET) => (),
            "--no-zip" => zip = false,
            "--without-calc" => calc = false,
            _ => {
                eprintln!("{USAGE}");
                return ExitCode::from(2);
            }
        }
    }
    let result = std::env::current_dir()
        .map_err(|e| format!("no current directory: {e}"))
        .and_then(|cwd| Workspace::load(&cwd))
        .and_then(|ws| {
            let out = out.unwrap_or_else(|| ws.root.join("target/onecalc-package"));
            package(&ws, &out, zip, calc)
        });
    match result {
        Ok(manifest) => {
            println!("ONECALC PACKAGE {}", manifest["folder"]);
            ExitCode::SUCCESS
        }
        Err(why) => {
            println!("ONECALC PACKAGE failed: {why}");
            ExitCode::from(1)
        }
    }
}

fn package(ws: &Workspace, out: &Path, zip: bool, calc: bool) -> Result<Value, String> {
    let io = |what: &str, path: &Path, e: std::io::Error| format!("{what} {}: {e}", path.display());
    fs::create_dir_all(out).map_err(|e| io("cannot create", out, e))?;
    let out = out
        .canonicalize()
        .map_err(|e| io("cannot resolve", out, e))?;
    let out = PathBuf::from(out.display().to_string().trim_start_matches(r"\\?\"));
    let app = &APPS[0];

    // 1. The page, packed.
    let page_dir = out.join("page");
    let _ = fs::remove_dir_all(&page_dir);
    let page_files = bundle(ws, app, &page_dir)?;
    let mut packed = Vec::new();
    for (name, _) in &page_files {
        let path = page_dir.join(name);
        packed.push((
            name.clone(),
            fs::read(&path).map_err(|e| io("cannot read", &path, e))?,
        ));
    }
    let block = out.join("page.block");
    fs::write(&block, assets::pack(&packed)).map_err(|e| io("cannot write", &block, e))?;
    // DNA Calc's page, for the companion.
    let calc_block = out.join("calc-page.block");
    let calc_files = if calc {
        let calc_dir = out.join("calc-page");
        let _ = fs::remove_dir_all(&calc_dir);
        let files = bundle(ws, &APPS[1], &calc_dir)?;
        let mut packed = Vec::new();
        for (name, _) in &files {
            let path = calc_dir.join(name);
            packed.push((
                name.clone(),
                fs::read(&path).map_err(|e| io("cannot read", &path, e))?,
            ));
        }
        fs::write(&calc_block, assets::pack(&packed))
            .map_err(|e| io("cannot write", &calc_block, e))?;
        files
    } else {
        Vec::new()
    };

    // 2. The executable and the companion, release, the C runtime static, the page embedded.
    println!("ONECALC PACKAGE building release (crt-static, pages embedded)");
    let mut build = Command::new(cargo::cargo_program());
    if !cfg!(windows) {
        build.arg("xwin");
    }
    build
        .current_dir(&ws.root)
        .args([
            "build",
            "--release",
            "--locked",
            "--target",
            WINDOWS_TARGET,
            "-p",
            "dna-onecalc-desktop",
            "-p",
            "calc-xll-shim",
        ])
        .env("RUSTFLAGS", "-C target-feature=+crt-static")
        .env("DNACALC_ONECALC_PAGE", &block);
    if calc {
        build
            .args(["-p", "dna-calc-desktop"])
            .env("DNACALC_CALC_PAGE", &calc_block);
    }
    let built = cargo::output(&mut build)?;
    if !built.status.success() {
        return Err(format!(
            "cargo build failed:\n{}",
            cargo::tail(&built.stderr, 40)
        ));
    }
    let release = ws.target_dir().join(WINDOWS_TARGET).join("release");
    let exe = fs::read(release.join(EXE)).map_err(|e| io("cannot read", &release.join(EXE), e))?;
    let shim = fs::read(release.join("xlcall32.dll"))
        .map_err(|e| io("cannot read", &release.join("xlcall32.dll"), e))?;
    let calc_exe = if calc {
        Some(
            fs::read(release.join(CALC_EXE))
                .map_err(|e| io("cannot read", &release.join(CALC_EXE), e))?,
        )
    } else {
        None
    };

    // 3. What the executable and the companion link to.
    let mut imports = json!({});
    let mut checked = vec![(EXE, &exe), ("XLCALL32.DLL", &shim)];
    if let Some(bytes) = &calc_exe {
        checked.push((CALC_EXE, bytes));
    }
    for (name, bytes) in checked {
        let list = pe_imports(bytes)?;
        if let Some(runtime) = list.iter().find(|dll| needs_redistributable(dll)) {
            return Err(format!(
                "{name} imports {runtime}: the folder would need the Visual C++ redistributable"
            ));
        }
        imports[name] = json!(list);
    }

    // 4. The folder, the zip and the record.
    let folder = out.join(FOLDER);
    let _ = fs::remove_dir_all(&folder);
    fs::create_dir_all(&folder).map_err(|e| io("cannot create", &folder, e))?;
    let licence = fs::read(ws.root.join("LICENSE")).map_err(|e| format!("LICENSE: {e}"))?;
    let mut files: Vec<(String, Vec<u8>)> = vec![(EXE.to_owned(), exe)];
    if let Some(bytes) = calc_exe {
        files.push((CALC_EXE.to_owned(), bytes));
    }
    files.extend([
        ("XLCALL32.DLL".to_owned(), shim),
        ("README.txt".to_owned(), readme(calc).into_bytes()),
        ("LICENSE".to_owned(), licence),
    ]);
    // DNA Calc on its own: the same executable and shim, its own README.
    let mut calc_listed = Vec::new();
    if calc {
        let own = out.join(CALC_FOLDER);
        let _ = fs::remove_dir_all(&own);
        fs::create_dir_all(&own).map_err(|e| io("cannot create", &own, e))?;
        let mut entries = Vec::new();
        for name in [CALC_EXE, "XLCALL32.DLL", "LICENSE"] {
            let bytes = files
                .iter()
                .find(|(n, _)| n == name)
                .map(|(_, b)| b.clone())
                .ok_or(format!("{name} missing"))?;
            entries.push((name.to_owned(), bytes));
        }
        entries.push(("README.txt".to_owned(), CALC_README.as_bytes().to_vec()));
        let mut bytes_total = 0;
        for (name, bytes) in &entries {
            fs::write(own.join(name), bytes).map_err(|e| io("cannot write", &own.join(name), e))?;
            bytes_total += bytes.len();
            calc_listed.push(json!({"name": name, "bytes": bytes.len(), "sha256": calc_verdict::sha256_hex(bytes)}));
        }
        println!("ONECALC PACKAGE calc folder {} bytes", bytes_total);
        if zip {
            let name = format!("{CALC_FOLDER}-{}-win-x64.zip", workspace_version(&ws.root)?);
            let zipped: Vec<(String, Vec<u8>)> = entries
                .iter()
                .map(|(n, b)| (format!("{CALC_FOLDER}/{n}"), b.clone()))
                .collect();
            let bytes = zip_bytes(&zipped);
            let path = out.join(&name);
            fs::write(&path, &bytes).map_err(|e| io("cannot write", &path, e))?;
            println!("ONECALC PACKAGE calc zip {name}: {} bytes", bytes.len());
        }
    }
    let mut listed = Vec::new();
    for (name, bytes) in &files {
        fs::write(folder.join(name), bytes)
            .map_err(|e| io("cannot write", &folder.join(name), e))?;
        listed.push(
            json!({"name": name, "bytes": bytes.len(), "sha256": calc_verdict::sha256_hex(bytes)}),
        );
    }
    let version = workspace_version(&ws.root)?;
    let mut manifest = json!({
        "folder": folder.display().to_string(),
        "version": version,
        "profile": "release",
        "target": WINDOWS_TARGET,
        "rustflags": "-C target-feature=+crt-static",
        "commit": git_head(&ws.root),
        "files": listed,
        "calc_files": calc_listed,
        "page": page_files.iter().map(|(name, size)| json!({"name": name, "bytes": size})).collect::<Vec<_>>(),
        "page_block_bytes": fs::metadata(&block).map(|m| m.len()).unwrap_or(0),
        "calc_page": calc_files.iter().map(|(name, size)| json!({"name": name, "bytes": size})).collect::<Vec<_>>(),
        "calc_page_block_bytes": fs::metadata(&calc_block).map(|m| m.len()).unwrap_or(0),
        "imports": imports,
    });
    let folder_bytes: usize = files.iter().map(|(_, b)| b.len()).sum();
    manifest["folder_bytes"] = json!(folder_bytes);
    if zip {
        let name = format!("{FOLDER}-{version}-win-x64.zip");
        let entries: Vec<(String, Vec<u8>)> = files
            .iter()
            .map(|(n, b)| (format!("{FOLDER}/{n}"), b.clone()))
            .collect();
        let bytes = zip_bytes(&entries);
        let path = out.join(&name);
        fs::write(&path, &bytes).map_err(|e| io("cannot write", &path, e))?;
        manifest["zip"] =
            json!({"name": name, "bytes": bytes.len(), "sha256": calc_verdict::sha256_hex(&bytes)});
    }
    let record = out.join("package-manifest.json");
    fs::write(
        &record,
        serde_json::to_string_pretty(&manifest).map_err(|e| e.to_string())?,
    )
    .map_err(|e| io("cannot write", &record, e))?;
    for file in manifest["files"].as_array().into_iter().flatten() {
        println!(
            "ONECALC PACKAGE file {}: {} bytes sha256 {}",
            file["name"], file["bytes"], file["sha256"]
        );
    }
    if zip {
        println!(
            "ONECALC PACKAGE zip {}: {} bytes sha256 {}",
            manifest["zip"]["name"], manifest["zip"]["bytes"], manifest["zip"]["sha256"]
        );
    }
    println!("ONECALC PACKAGE record {}", record.display());
    Ok(manifest)
}

/// The folder's README: the line about DNA Calc names it only when it is in the folder.
fn readme(calc: bool) -> String {
    if calc {
        README.to_owned()
    } else {
        README
            .replace("  dna-calc-desktop.exe     DNA Calc, the workbook application; its page is inside it\r\n", "")
            .replace("when dna-calc-desktop.exe is in this folder", "when dna-calc-desktop.exe is put in this folder")
    }
}

/// The import table's DLL names of a PE image (lower case).
pub fn pe_imports(image: &[u8]) -> Result<Vec<String>, String> {
    let bad = |what: &str| format!("not a 64-bit PE image: {what}");
    let u16_at = |at: usize| -> Result<usize, String> {
        image
            .get(at..at + 2)
            .map(|b| u16::from_le_bytes([b[0], b[1]]) as usize)
            .ok_or_else(|| bad("short"))
    };
    let u32_at = |at: usize| -> Result<usize, String> {
        image
            .get(at..at + 4)
            .map(|b| u32::from_le_bytes([b[0], b[1], b[2], b[3]]) as usize)
            .ok_or_else(|| bad("short"))
    };
    if image.get(..2) != Some(b"MZ") {
        return Err(bad("no MZ header"));
    }
    let pe = u32_at(0x3c)?;
    if image.get(pe..pe + 4) != Some(b"PE\0\0") {
        return Err(bad("no PE signature"));
    }
    let sections = u16_at(pe + 6)?;
    let optional_size = u16_at(pe + 20)?;
    let optional = pe + 24;
    if u16_at(optional)? != 0x20b {
        return Err(bad("not PE32+"));
    }
    // Data directory 1 is the import table.
    let import_rva = u32_at(optional + 112 + 8)?;
    let table = optional + optional_size;
    let to_offset = |rva: usize| -> Result<usize, String> {
        for section in 0..sections {
            let header = table + section * 40;
            let (virtual_size, virtual_address, raw_size, raw_pointer) = (
                u32_at(header + 8)?,
                u32_at(header + 12)?,
                u32_at(header + 16)?,
                u32_at(header + 20)?,
            );
            if rva >= virtual_address && rva < virtual_address + virtual_size.max(raw_size) {
                return Ok(raw_pointer + rva - virtual_address);
            }
        }
        Err(bad("an address outside every section"))
    };
    if import_rva == 0 {
        return Ok(Vec::new());
    }
    let mut names = Vec::new();
    let mut descriptor = to_offset(import_rva)?;
    loop {
        let (name_rva, first_thunk) = (u32_at(descriptor + 12)?, u32_at(descriptor + 16)?);
        if name_rva == 0 && first_thunk == 0 {
            break;
        }
        let at = to_offset(name_rva)?;
        let end = image[at..]
            .iter()
            .position(|&b| b == 0)
            .ok_or_else(|| bad("an unterminated name"))?;
        names.push(String::from_utf8_lossy(&image[at..at + end]).to_ascii_lowercase());
        descriptor += 20;
    }
    Ok(names)
}

/// Whether a DLL is part of the Visual C++ redistributable (the universal C runtime, `ucrtbase` and
/// the `api-ms-win-crt` forwarders, ships with Windows 10 and later).
pub fn needs_redistributable(dll: &str) -> bool {
    dll.starts_with("vcruntime")
        || dll.starts_with("msvcp")
        || dll.starts_with("vcomp")
        || dll.starts_with("concrt")
        || dll.starts_with("msvcr")
}

fn workspace_version(root: &Path) -> Result<String, String> {
    let text =
        fs::read_to_string(root.join("Cargo.toml")).map_err(|e| format!("Cargo.toml: {e}"))?;
    let section = text
        .split("[workspace.package]")
        .nth(1)
        .ok_or("Cargo.toml has no [workspace.package]")?;
    section
        .lines()
        .find_map(|line| line.trim().strip_prefix("version"))
        .and_then(|rest| rest.split('"').nth(1))
        .map(str::to_owned)
        .ok_or_else(|| "[workspace.package] has no version".to_owned())
}

fn git_head(root: &Path) -> String {
    let head = cargo::git(root, &["rev-parse", "HEAD"])
        .unwrap_or_default()
        .trim()
        .to_owned();
    let dirty = cargo::git(root, &["status", "--porcelain", "--untracked-files=no"])
        .map(|s| !s.trim().is_empty())
        .unwrap_or(false);
    if dirty {
        format!("{head}+uncommitted")
    } else {
        head
    }
}

/// The CRC-32 (IEEE) of `bytes`, as a zip stores it.
pub fn crc32(bytes: &[u8]) -> u32 {
    let mut table = [0u32; 256];
    for (n, entry) in table.iter_mut().enumerate() {
        let mut c = n as u32;
        for _ in 0..8 {
            c = if c & 1 != 0 {
                0xedb8_8320 ^ (c >> 1)
            } else {
                c >> 1
            };
        }
        *entry = c;
    }
    !bytes.iter().fold(!0u32, |c, &b| {
        table[((c ^ u32::from(b)) & 0xff) as usize] ^ (c >> 8)
    })
}

/// A zip of `entries` (path inside the zip, bytes), deflated, with a fixed time stamp so that the
/// same files give the same zip.
pub fn zip_bytes(entries: &[(String, Vec<u8>)]) -> Vec<u8> {
    // 2026-01-01 00:00:00 in DOS date and time.
    const DOS_TIME: u16 = 0;
    const DOS_DATE: u16 = ((2026 - 1980) << 9) | (1 << 5) | 1;
    let mut out = Vec::new();
    let mut central = Vec::new();
    for (name, data) in entries {
        let packed = miniz_oxide::deflate::compress_to_vec(data, 9);
        let (method, body): (u16, &[u8]) = if packed.len() < data.len() {
            (8, &packed)
        } else {
            (0, data)
        };
        let crc = crc32(data);
        let offset = out.len() as u32;
        let mut header = Vec::new();
        header.extend_from_slice(&20u16.to_le_bytes()); // version needed
        header.extend_from_slice(&0x0800u16.to_le_bytes()); // flags: UTF-8 names
        header.extend_from_slice(&method.to_le_bytes());
        header.extend_from_slice(&DOS_TIME.to_le_bytes());
        header.extend_from_slice(&DOS_DATE.to_le_bytes());
        header.extend_from_slice(&crc.to_le_bytes());
        header.extend_from_slice(&(body.len() as u32).to_le_bytes());
        header.extend_from_slice(&(data.len() as u32).to_le_bytes());
        header.extend_from_slice(&(name.len() as u16).to_le_bytes());
        header.extend_from_slice(&0u16.to_le_bytes()); // extra length
        out.extend_from_slice(&0x0403_4b50u32.to_le_bytes());
        out.extend_from_slice(&header);
        out.extend_from_slice(name.as_bytes());
        out.extend_from_slice(body);
        central.extend_from_slice(&0x0201_4b50u32.to_le_bytes());
        central.extend_from_slice(&0x031eu16.to_le_bytes()); // made by: Unix, 3.0
        central.extend_from_slice(&header);
        central.extend_from_slice(&0u16.to_le_bytes()); // comment length
        central.extend_from_slice(&0u16.to_le_bytes()); // disk number
        central.extend_from_slice(&0u16.to_le_bytes()); // internal attributes
        central.extend_from_slice(&(0o100644u32 << 16).to_le_bytes()); // external attributes
        central.extend_from_slice(&offset.to_le_bytes());
        central.extend_from_slice(name.as_bytes());
    }
    let start = out.len() as u32;
    out.extend_from_slice(&central);
    out.extend_from_slice(&0x0605_4b50u32.to_le_bytes());
    out.extend_from_slice(&0u16.to_le_bytes());
    out.extend_from_slice(&0u16.to_le_bytes());
    out.extend_from_slice(&(entries.len() as u16).to_le_bytes());
    out.extend_from_slice(&(entries.len() as u16).to_le_bytes());
    out.extend_from_slice(&(central.len() as u32).to_le_bytes());
    out.extend_from_slice(&start.to_le_bytes());
    out.extend_from_slice(&0u16.to_le_bytes());
    out
}

/// The entries of a zip (path, bytes), read through its central directory; stored and deflated
/// entries, each checked against its CRC-32.
pub fn unzip(zip: &[u8]) -> Result<Vec<(String, Vec<u8>)>, String> {
    let bad = |what: &str| format!("not a zip this tool reads: {what}");
    let u16_at = |at: usize| -> Result<usize, String> {
        zip.get(at..at + 2)
            .map(|b| u16::from_le_bytes([b[0], b[1]]) as usize)
            .ok_or_else(|| bad("short"))
    };
    let u32_at = |at: usize| -> Result<usize, String> {
        zip.get(at..at + 4)
            .map(|b| u32::from_le_bytes([b[0], b[1], b[2], b[3]]) as usize)
            .ok_or_else(|| bad("short"))
    };
    let end = zip.len().checked_sub(22).ok_or_else(|| bad("too short"))?;
    if u32_at(end)? != 0x0605_4b50 {
        return Err(bad("no end record"));
    }
    let (count, mut at) = (u16_at(end + 10)?, u32_at(end + 16)?);
    let mut entries = Vec::new();
    for _ in 0..count {
        if u32_at(at)? != 0x0201_4b50 {
            return Err(bad("no central entry"));
        }
        let (method, crc, packed, size) = (
            u16_at(at + 10)?,
            u32_at(at + 16)?,
            u32_at(at + 20)?,
            u32_at(at + 24)?,
        );
        let (name_len, extra, comment, local) = (
            u16_at(at + 28)?,
            u16_at(at + 30)?,
            u16_at(at + 32)?,
            u32_at(at + 42)?,
        );
        let name = String::from_utf8_lossy(
            zip.get(at + 46..at + 46 + name_len)
                .ok_or_else(|| bad("short"))?,
        )
        .into_owned();
        let data_at = local + 30 + u16_at(local + 26)? + u16_at(local + 28)?;
        let body = zip
            .get(data_at..data_at + packed)
            .ok_or_else(|| bad("short"))?;
        let data = match method {
            0 => body.to_vec(),
            8 => miniz_oxide::inflate::decompress_to_vec(body)
                .map_err(|e| format!("{name}: {e:?}"))?,
            other => return Err(bad(&format!("method {other}"))),
        };
        if data.len() != size || crc32(&data) as usize != crc {
            return Err(format!("{name}: size or CRC-32 differs"));
        }
        entries.push((name, data));
        at += 46 + name_len + extra + comment;
    }
    Ok(entries)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Native-only: a zip of files reads back byte for byte (deflated and stored entries), the same
    /// files give the same zip, and the CRC-32 is the standard one.
    #[test]
    fn the_zip_holds_its_files_exactly() {
        assert_eq!(crc32(b"123456789"), 0xcbf4_3926);
        let entries = vec![
            (
                "DNA-OneCalc/a.txt".to_owned(),
                b"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa".to_vec(),
            ),
            (
                "DNA-OneCalc/b.bin".to_owned(),
                (0u32..5000)
                    .map(|i| (i.wrapping_mul(2_654_435_761) >> 13) as u8)
                    .collect(),
            ),
            ("DNA-OneCalc/empty".to_owned(), Vec::new()),
        ];
        let zip = zip_bytes(&entries);
        assert_eq!(unzip(&zip).unwrap(), entries);
        assert_eq!(zip_bytes(&entries), zip);
    }

    /// Native-only: the import table of a real executable (this test's own) names the system DLLs it
    /// links to, and the redistributable test knows the C++ runtime's names.
    #[test]
    fn a_real_executables_imports_are_read() {
        let exe = fs::read(std::env::current_exe().unwrap()).unwrap();
        #[cfg(windows)]
        {
            let imports = pe_imports(&exe).unwrap();
            assert!(
                imports.iter().any(|dll| dll == "kernel32.dll"),
                "{imports:?}"
            );
        }
        // Elsewhere the running executable is not a PE image: a typed error, never a panic.
        #[cfg(not(windows))]
        assert!(
            pe_imports(&exe)
                .unwrap_err()
                .starts_with("not a 64-bit PE image")
        );
        assert!(pe_imports(b"not an image").is_err());
        assert!(needs_redistributable("vcruntime140.dll") && needs_redistributable("msvcp140.dll"));
        assert!(
            !needs_redistributable("kernel32.dll")
                && !needs_redistributable("api-ms-win-crt-heap-l1-1-0.dll")
        );
    }

    /// Native-only: the version comes from the workspace package table.
    #[test]
    fn the_workspace_version_is_read() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
        assert!(workspace_version(&root).unwrap().split('.').count() >= 2);
    }

    /// Native-only: both folders' READMEs name the runtime download and the file each starts, and
    /// DNA Calc's names no OneCalc file.
    #[test]
    fn the_readmes_name_their_program_and_the_runtime() {
        const CRLF: &str = "\r\n";
        for (text, exe) in [(readme(true), EXE), (CALC_README.to_owned(), CALC_EXE)] {
            assert!(text.contains(exe));
            assert!(text.contains("https://developer.microsoft.com/microsoft-edge/webview2/"));
            assert!(text.contains(CRLF));
        }
        assert!(!CALC_README.contains(EXE));
    }
}
