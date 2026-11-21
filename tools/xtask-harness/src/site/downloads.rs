use super::content::{create_directory, markdown, write_page};
use crate::package::{CALC_FOLDER, FOLDER};
use serde::Deserialize;
use std::collections::BTreeMap;
use std::fmt::Write;
use std::fs;
use std::path::Path;
use std::process::ExitCode;

pub(crate) const USAGE: &str = "usage: cargo xtask release notes --manifest <installer-manifest.json> --draft <notes.md> --out <dir>";

const WEBVIEW2: &str = "Both Windows apps need the Microsoft Edge WebView2 Runtime. If the installer reports that the runtime is missing, install Microsoft's Evergreen Runtime and rerun the installer.";
const PENDING: &str = "Windows installer downloads will appear after signing and clean-machine installation verification.";

#[derive(Deserialize)]
struct Manifest {
    version: String,
    commit: String,
    signed: bool,
    signer: Option<String>,
    products: Vec<Product>,
    artifacts: Vec<Artifact>,
}

#[derive(Deserialize)]
struct Product {
    folder: String,
    msi: String,
}

#[derive(Deserialize)]
struct Artifact {
    name: String,
    bytes: u64,
    sha256: String,
}

impl Manifest {
    fn installers(&self) -> Result<Vec<(&'static str, &Artifact)>, String> {
        let mut version = self.version.split('.');
        if !(0..3).all(|_| {
            version
                .next()
                .is_some_and(|part| !part.is_empty() && part.bytes().all(|b| b.is_ascii_digit()))
        }) || version.next().is_some()
            || self.commit.len() != 40
            || !self.commit.bytes().all(|b| b.is_ascii_hexdigit())
        {
            return Err(
                "the installer manifest needs a numeric version and full Git commit".into(),
            );
        }
        if self.products.len() != 2 {
            return Err("the manifest must list the two Windows products".into());
        }
        [(CALC_FOLDER, "DNA Calc"), (FOLDER, "DNA OneCalc")]
            .into_iter()
            .map(|(folder, title)| {
                let mut products = self.products.iter().filter(|p| p.folder == folder);
                let product = products
                    .next()
                    .ok_or_else(|| format!("missing product {folder}"))?;
                if products.next().is_some()
                    || !product.msi.ends_with(".msi")
                    || !product
                        .msi
                        .bytes()
                        .all(|b| b.is_ascii_alphanumeric() || b".-_".contains(&b))
                    || !product
                        .msi
                        .strip_prefix(folder)
                        .is_some_and(|suffix| suffix.starts_with('-'))
                {
                    return Err(format!("invalid installer record for {folder}"));
                }
                let mut artifacts = self.artifacts.iter().filter(|a| a.name == product.msi);
                let artifact = artifacts
                    .next()
                    .ok_or_else(|| format!("missing artifact {}", product.msi))?;
                if artifacts.next().is_some() {
                    return Err(format!("duplicate artifact {}", product.msi));
                }
                Ok((title, artifact))
            })
            .collect()
    }
}

impl Artifact {
    fn verify(&self, bytes: &[u8]) -> Result<(), String> {
        let hash = calc_verdict::sha256_hex(bytes);
        if bytes.len() as u64 != self.bytes || !self.sha256.eq_ignore_ascii_case(&hash) {
            return Err(format!(
                "{} differs from the manifest size or SHA-256",
                self.name
            ));
        }
        Ok(())
    }
}

fn build(manifest_path: &Path, draft: &Path, output: &Path) -> Result<usize, String> {
    let manifest: Manifest = serde_json::from_slice(&super::read(manifest_path)?)
        .map_err(|e| format!("{}: {e}", manifest_path.display()))?;
    let installers = manifest.installers()?;
    let draft = fs::read_to_string(draft).map_err(|e| format!("{}: {e}", draft.display()))?;
    let heading = format!("DNA Calc {} Windows installers", manifest.version);
    let mut notes = format!(
        "# {heading}\n\nSource commit `{}`\n\n{}\n\n## Downloads\n\n",
        manifest.commit,
        draft.trim()
    );
    let mut files = Vec::new();
    if manifest.signed {
        let signer = manifest
            .signer
            .as_deref()
            .map(str::trim)
            .filter(|name| !name.is_empty() && !name.contains(['\r', '\n']))
            .ok_or("a signed installer manifest needs the verified signer name")?;
        let folder = manifest_path
            .parent()
            .ok_or("installer manifest has no parent directory")?;
        for (product, artifact) in installers {
            let bytes = super::read(&folder.join(&artifact.name))?;
            artifact.verify(&bytes)?;
            let _ = writeln!(
                notes,
                "- [{product}](./{}) for Windows x64. {} bytes. SHA-256 `{}`.",
                artifact.name,
                artifact.bytes,
                artifact.sha256.to_ascii_lowercase()
            );
            files.push((&artifact.name, bytes));
        }
        let _ = writeln!(notes, "\nSigned by {}.", super::content::escape(signer));
    } else {
        let _ = writeln!(notes, "{PENDING}");
    }
    let _ = writeln!(
        notes,
        "\n## Installing and uninstalling\n\n{WEBVIEW2}\n\nTo uninstall, open Windows Settings, go to Apps, find the application under Installed apps and select Uninstall."
    );
    let directory = output.join("download");
    create_directory(&directory)?;
    for (name, bytes) in &files {
        super::write(&directory.join(name), bytes)?;
    }
    super::write(&directory.join("release-notes.md"), notes.as_bytes())?;
    let contents = format!(
        "{}<footer><a href=\"release-notes.md\">Release notes in Markdown</a></footer>",
        markdown(&notes)
    );
    write_page(&directory, "index.html", &heading, "", &contents)?;
    Ok(files.len())
}

pub(crate) fn run(args: &[String]) -> ExitCode {
    if args.first().map(String::as_str) != Some("notes") || args.len() != 7 {
        eprintln!("{USAGE}");
        return ExitCode::from(2);
    }
    let options: BTreeMap<&str, &Path> = args[1..]
        .chunks_exact(2)
        .map(|pair| (pair[0].as_str(), Path::new(&pair[1])))
        .collect();
    let (Some(manifest), Some(draft), Some(output)) = (
        options.get("--manifest"),
        options.get("--draft"),
        options.get("--out"),
    ) else {
        eprintln!("{USAGE}");
        return ExitCode::from(2);
    };
    match build(manifest, draft, output) {
        Ok(files) => {
            println!(
                "RELEASE NOTES {} ({} signed installer files)",
                output.join("download").display(),
                files
            );
            ExitCode::SUCCESS
        }
        Err(error) => {
            eprintln!("xtask release notes: {error}");
            ExitCode::from(1)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn installer_verification_checks_size_and_digest() {
        let bytes = b"test installer";
        let artifact = Artifact {
            name: "test.msi".into(),
            bytes: bytes.len() as u64,
            sha256: calc_verdict::sha256_hex(bytes),
        };
        assert!(artifact.verify(bytes).is_ok());
        assert!(artifact.verify(b"TEST INSTALLER").is_err());
        assert!(artifact.verify(b"").is_err());
    }
}
