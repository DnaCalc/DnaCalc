//! Whether a workbook file's own VBA project may run (dc-0f1k.22).
//!
//! Excel's default, which DNA OneCalc follows (docs/architecture/ONECALC_ADDINS.md, Trust): a
//! macro workbook opens with its project held and saved back but not compiled, and nothing in it,
//! `Workbook_Open` included, runs until the person enables macros for that file. A file with the
//! Mark of the Web (a `Zone.Identifier` stream whose ZoneId is 3, internet, or 4, restricted) or
//! one opened from the network cannot be enabled at all.
//!
//! Every route here that compiles or runs a file's project ([`crate::prepare_file_project`] and
//! the others) takes a [`ProjectGrant`], and a grant has exactly two origins: the person enabled
//! macros for a file whose origin does not block them ([`ProjectGrant::person_enabled`]), or an
//! evidence run lists the file's exact bytes in its trusted-code manifest
//! ([`ProjectGrant::evidence_manifest`]). Opening a file never produces one.

/// Where an opened workbook file came from, as far as its host can tell.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FileOrigin {
    /// The host cannot tell (a browser's file input reads no path and no Mark of the Web).
    Unknown,
    /// A local file without a Mark of the Web.
    Local,
    /// A file whose Mark of the Web names this URL security zone (0 to 4).
    Zone(u32),
    /// A file opened from a network path (`\\server\share\...`).
    Network,
}

/// Why a file's project cannot be enabled.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MacroBlock {
    /// The Mark of the Web names the internet (3) or restricted (4) zone.
    Zone(u32),
    /// The file was opened from the network.
    Network,
}

impl FileOrigin {
    /// What stops the person enabling the file's project, if anything: zones 3 and 4, and the
    /// network. Zones 0 to 2 (this computer, the intranet, trusted sites) and unmarked files open
    /// disabled and can be enabled.
    pub fn block(self) -> Option<MacroBlock> {
        match self {
            Self::Zone(zone @ (3 | 4)) => Some(MacroBlock::Zone(zone)),
            Self::Network => Some(MacroBlock::Network),
            Self::Zone(_) | Self::Local | Self::Unknown => None,
        }
    }
}

impl std::fmt::Display for MacroBlock {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Zone(zone) => write!(
                f,
                "This file carries the Mark of the Web (zone {zone}: it came from the internet or another computer), so its VBA project is not run. Save a copy you trust and open that"
            ),
            Self::Network => f.write_str(
                "This file was opened from the network, so its VBA project is not run. Copy it to this computer and open the copy",
            ),
        }
    }
}

/// Permission to compile a workbook file's own VBA project and raise its events. Only the two
/// constructors below make one; nothing derives it from opening a file.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ProjectGrant {
    reason: GrantReason,
}

/// Why a [`ProjectGrant`] was given.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum GrantReason {
    /// The person enabled macros for this file (the page's bar, or the desktop harness's
    /// `--enable-macros`).
    PersonEnabled,
    /// An evidence run's trusted-code manifest (its SHA-256) lists the package (its SHA-256)
    /// with its VBA project enabled.
    EvidenceManifest {
        manifest_sha256: String,
        package_sha256: String,
    },
}

impl ProjectGrant {
    /// The person enabled macros for a file of `origin`; refused for a file whose origin blocks
    /// them, whatever the person asks.
    pub fn person_enabled(origin: FileOrigin) -> Result<Self, MacroBlock> {
        match origin.block() {
            Some(block) => Err(block),
            None => Ok(Self {
                reason: GrantReason::PersonEnabled,
            }),
        }
    }

    /// An evidence run's trusted-code manifest lists the package. The caller has checked the
    /// package's bytes against `package_sha256` and found the manifest entry enabling its project.
    pub fn evidence_manifest(manifest_sha256: &str, package_sha256: &str) -> Self {
        Self {
            reason: GrantReason::EvidenceManifest {
                manifest_sha256: manifest_sha256.to_owned(),
                package_sha256: package_sha256.to_owned(),
            },
        }
    }

    pub fn reason(&self) -> &GrantReason {
        &self.reason
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Zones 3 and 4 and the network block; every other origin can be enabled.
    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn marked_and_network_files_are_never_granted() {
        for (origin, block) in [
            (FileOrigin::Zone(3), Some(MacroBlock::Zone(3))),
            (FileOrigin::Zone(4), Some(MacroBlock::Zone(4))),
            (FileOrigin::Network, Some(MacroBlock::Network)),
            (FileOrigin::Zone(0), None),
            (FileOrigin::Zone(1), None),
            (FileOrigin::Zone(2), None),
            (FileOrigin::Local, None),
            (FileOrigin::Unknown, None),
        ] {
            assert_eq!(origin.block(), block, "{origin:?}");
            assert_eq!(ProjectGrant::person_enabled(origin).err(), block);
        }
        assert_eq!(
            ProjectGrant::person_enabled(FileOrigin::Local).unwrap().reason(),
            &GrantReason::PersonEnabled
        );
    }
}
