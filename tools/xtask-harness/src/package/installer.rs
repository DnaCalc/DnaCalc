//! Linux wixl packaging of the same folders the portable package command assembles.
use super::*;

const PRODUCTS: [(&str, &str, &str); 2] = [
    (CALC_FOLDER, "dna-calc", "calc_files"),
    (FOLDER, "dna-onecalc", "files"),
];

pub(super) fn run(raw: &[String]) -> ExitCode {
    match execute(raw) {
        Ok(()) => ExitCode::SUCCESS,
        Err(why) => {
            eprintln!("INSTALLER PACKAGE failed: {why}\n{USAGE}");
            ExitCode::from(1)
        }
    }
}

fn execute(raw: &[String]) -> Result<(), String> {
    let mut package_dir = None;
    let mut out = None;
    let mut wixl = std::env::var_os("WIXL").unwrap_or_else(|| "wixl".into());
    let mut args = raw.iter();
    while let Some(arg) = args.next() {
        let value = args.next().ok_or_else(|| format!("{arg} needs a value"))?;
        match arg.as_str() {
            "--package" => package_dir = Some(PathBuf::from(value)),
            "--out" => out = Some(PathBuf::from(value)),
            "--wixl" => wixl = value.into(),
            _ => return Err(format!("unknown option {arg}")),
        }
    }
    let cwd = std::env::current_dir().map_err(|e| e.to_string())?;
    let ws = Workspace::load(&cwd)?;
    let out = out.unwrap_or_else(|| ws.root.join("target/windows-msi"));
    let package_dir = match package_dir {
        Some(path) => path,
        None => {
            let path = ws.root.join("target/onecalc-package");
            package(&ws, &path, false, true)?;
            path
        }
    };
    let package_dir = package_dir.canonicalize().map_err(|e| e.to_string())?;
    let manifest_bytes = fs::read(package_dir.join("package-manifest.json"))
        .map_err(|e| format!("package-manifest.json: {e}"))?;
    let manifest: Value = serde_json::from_slice(&manifest_bytes).map_err(|e| e.to_string())?;
    let (version, commit) = identity(&manifest)?;
    // Validate every payload before producing either MSI. The manifest is also the signing handoff.
    for (folder, _, field) in PRODUCTS {
        verify_payload(&package_dir.join(folder), &manifest[field], folder)?;
    }
    fs::create_dir_all(&out).map_err(|e| e.to_string())?;
    let out = out.canonicalize().map_err(|e| e.to_string())?;
    let mut artifacts = Vec::new();
    let mut products = Vec::new();
    for (folder, source, _) in PRODUCTS {
        let name = format!("{folder}-{version}-win-x64.msi");
        let code = product_code(folder, &version);
        let built = cargo::output(
            Command::new(&wixl)
                .current_dir(&ws.root)
                .args([
                    "-a",
                    "x64",
                    "-D",
                    &format!("Version={version}"),
                    "-D",
                    &format!("Commit={commit}"),
                ])
                .args(["-D", &format!("ProductCode={code}")])
                .arg("-D")
                .arg(format!("Src={}", package_dir.join(folder).display()))
                .arg("-D")
                .arg(format!(
                    "Icon=apps/{source}-desktop/icon/{}.ico",
                    if source == "dna-calc" {
                        "calc"
                    } else {
                        "onecalc"
                    }
                ))
                .arg("-o")
                .arg(out.join(&name))
                .arg(ws.root.join(format!("deploy/windows/{source}.wxs"))),
        )?;
        if !built.status.success() {
            return Err(format!("wixl {folder}: {}", cargo::text(&built.stderr)));
        }
        if !built.stderr.is_empty() {
            eprintln!("{}", cargo::text(&built.stderr));
        }
        artifacts.push(record_file(&out, &name)?);
        products.push(json!({"folder": folder, "msi": name, "product_code": code}));
    }
    fs::write(
        out.join("Install.ps1"),
        include_str!("../../../../deploy/windows/Install.ps1"),
    )
    .map_err(|e| e.to_string())?;
    artifacts.push(record_file(&out, "Install.ps1")?);
    let record = json!({
        "version": version, "source_version": manifest["version"], "commit": commit, "target": WINDOWS_TARGET,
        "signed": false, "products": products, "artifacts": artifacts,
        "package_manifest_sha256": calc_verdict::sha256_hex(&manifest_bytes),
        "inputs": {FOLDER: manifest["files"], CALC_FOLDER: manifest["calc_files"]},
    });
    fs::write(
        out.join("installer-manifest.json"),
        serde_json::to_vec_pretty(&record).map_err(|e| e.to_string())?,
    )
    .map_err(|e| e.to_string())?;
    artifacts.push(record_file(&out, "installer-manifest.json")?);
    let sums = artifacts
        .iter()
        .map(|row| {
            format!(
                "{}  {}\n",
                row["sha256"].as_str().unwrap(),
                row["name"].as_str().unwrap()
            )
        })
        .collect::<String>();
    fs::write(out.join("SHA256SUMS"), sums).map_err(|e| e.to_string())?;
    for row in artifacts {
        println!(
            "INSTALLER PACKAGE {}: {} bytes sha256 {}",
            row["name"], row["bytes"], row["sha256"]
        );
    }
    println!("INSTALLER PACKAGE unsigned artifacts: {}", out.display());
    Ok(())
}

fn identity(manifest: &Value) -> Result<(String, String), String> {
    if manifest["target"] != WINDOWS_TARGET || manifest["profile"] != "release" {
        return Err("installer needs a release x86_64-pc-windows-msvc package".into());
    }
    let version = manifest["version"]
        .as_str()
        .ok_or("manifest has no version")?;
    // MSI compares three numeric fields only; preserve the full version in the provenance record.
    let numeric = version.split(['-', '+']).next().unwrap_or("");
    let fields = numeric
        .split('.')
        .map(str::parse::<u32>)
        .collect::<Result<Vec<_>, _>>()
        .map_err(|_| "version must begin with MAJOR.MINOR.PATCH")?;
    if fields.len() != 3 || fields[0] > 255 || fields[1] > 255 || fields[2] > 65535 {
        return Err("MSI version limits: MAJOR <= 255, MINOR <= 255, PATCH <= 65535".into());
    }
    let commit = manifest["commit"]
        .as_str()
        .ok_or("manifest has no commit")?;
    let hash = commit.strip_suffix("+uncommitted").unwrap_or(commit);
    if hash.len() != 40 || !hash.bytes().all(|b| b.is_ascii_hexdigit()) {
        return Err("manifest commit must be a full git hash (optionally +uncommitted)".into());
    }
    Ok((
        fields
            .iter()
            .map(u32::to_string)
            .collect::<Vec<_>>()
            .join("."),
        commit.to_owned(),
    ))
}

fn product_code(folder: &str, version: &str) -> String {
    // Stable product/version identity. A new version gets a new ProductCode; UpgradeCode stays fixed.
    let hash = calc_verdict::sha256_hex(format!("dnacalc-msi:{folder}:{version}").as_bytes());
    format!(
        "{}-{}-8{}-a{}-{}",
        &hash[..8],
        &hash[8..12],
        &hash[13..16],
        &hash[17..20],
        &hash[20..32]
    )
    .to_uppercase()
}

fn verify_payload(dir: &Path, rows: &Value, folder: &str) -> Result<(), String> {
    let mut expected = vec![CALC_EXE, "XLCALL32.DLL", "README.txt", "LICENSE"];
    if folder == FOLDER {
        expected.push(EXE);
    }
    let rows = rows
        .as_array()
        .ok_or_else(|| format!("manifest has no payload for {folder}"))?;
    let mut names = Vec::new();
    for row in rows {
        let name = row["name"].as_str().ok_or("file has no name")?;
        if !expected.contains(&name) || names.contains(&name) {
            return Err(format!("unexpected or duplicate {folder} payload: {name}"));
        }
        let actual = record_file(dir, name)?;
        if actual != *row {
            return Err(format!(
                "{folder}/{name} differs from package-manifest.json"
            ));
        }
        names.push(name);
    }
    if names.len() != expected.len() {
        return Err(format!("incomplete {folder} payload"));
    }
    let count = fs::read_dir(dir).map_err(|e| e.to_string())?.count();
    if count != expected.len() {
        return Err(format!("unlisted files in {folder}"));
    }
    Ok(())
}

fn record_file(dir: &Path, name: &str) -> Result<Value, String> {
    let bytes = fs::read(dir.join(name)).map_err(|e| format!("{name}: {e}"))?;
    Ok(json!({"name": name, "bytes": bytes.len(), "sha256": calc_verdict::sha256_hex(&bytes)}))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Native-only: validates package provenance and the MSI's numeric limits before invoking wixl.
    #[test]
    fn manifest_identity_rejects_non_windows_and_invalid_versions() {
        let mut manifest = json!({"version": "0.1.1-dev+abc", "commit": "0123456789abcdef0123456789abcdef01234567", "target": WINDOWS_TARGET, "profile": "release"});
        assert_eq!(identity(&manifest).unwrap().0, "0.1.1");
        for bad in [
            "1.2",
            "1.2.3.4",
            "256.0.0",
            "0.256.0",
            "0.0.65536",
            "../x",
            "a.b.c",
        ] {
            manifest["version"] = json!(bad);
            assert!(identity(&manifest).is_err(), "{bad}");
        }
        manifest["version"] = json!("0.1.0");
        manifest["target"] = json!("x86_64-unknown-linux-gnu");
        assert!(identity(&manifest).is_err());
        manifest["target"] = json!(WINDOWS_TARGET);
        manifest["commit"] = json!("unknown");
        assert!(identity(&manifest).is_err());
    }

    /// Native-only: upgrades need distinct product identities and a stable identity on rebuild.
    #[test]
    fn product_identity_is_stable_and_separate_for_products_and_versions() {
        let code = product_code(FOLDER, "0.1.0");
        assert_eq!(code, product_code(FOLDER, "0.1.0"));
        assert_ne!(code, product_code(FOLDER, "0.1.1"));
        assert_ne!(code, product_code(CALC_FOLDER, "0.1.0"));
        assert_eq!(code.len(), 36);
    }

    /// Native-only: reads real staging files, detects tampering, missing and unlisted payloads.
    #[test]
    fn payload_must_match_the_manifest_exactly() {
        let dir =
            std::env::temp_dir().join(format!("dnacalc-installer-test-{}", std::process::id()));
        fs::create_dir_all(&dir).unwrap();
        let mut rows = Vec::new();
        for name in [CALC_EXE, "XLCALL32.DLL", "README.txt", "LICENSE"] {
            fs::write(dir.join(name), name).unwrap();
            rows.push(record_file(&dir, name).unwrap());
        }
        let rows = json!(rows);
        verify_payload(&dir, &rows, CALC_FOLDER).unwrap();
        fs::write(dir.join("extra"), "unexpected").unwrap();
        assert!(verify_payload(&dir, &rows, CALC_FOLDER).is_err());
        fs::remove_file(dir.join("extra")).unwrap();
        fs::write(dir.join("LICENSE"), "changed").unwrap();
        assert!(verify_payload(&dir, &rows, CALC_FOLDER).is_err());
        fs::remove_file(dir.join("LICENSE")).unwrap();
        assert!(verify_payload(&dir, &rows, CALC_FOLDER).is_err());
        assert!(verify_payload(&dir, &json!([{ "name": "../outside" }]), CALC_FOLDER).is_err());
        fs::remove_dir_all(&dir).unwrap();
    }
}
