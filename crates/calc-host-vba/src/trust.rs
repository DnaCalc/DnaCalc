//! Whether a workbook file's own VBA project may run (dc-0f1k.22).
//!
//! Excel's default, which DNA OneCalc follows (docs/architecture/ONECALC_ADDINS.md, Trust): a
//! macro workbook opens with its project held and saved back but not compiled, and nothing in it,
//! `Workbook_Open` included, runs until the person enables macros for that file. A file with the
//! Mark of the Web (a `Zone.Identifier` stream whose ZoneId is 3, internet, or 4, restricted) or
//! one opened from the network cannot be enabled by the plain Enable: the person unblocks it
//! ([`ProjectGrant::person_unblocked`], behind a short warning), or the macro security level Enable
//! all trusts it ([`ProjectGrant::trust_all`], dc-15dk).
//!
//! Every route here that compiles or runs a file's project ([`crate::prepare_file_project`] and
//! the others) takes a [`ProjectGrant`], and a grant has these origins only: the person enabled
//! macros for a file whose origin does not block them ([`ProjectGrant::person_enabled`]) or
//! unblocked one, the macro security level or its pin trusts every file, or an evidence run lists
//! the file's exact bytes in its trusted-code manifest ([`ProjectGrant::evidence_manifest`]).
//! Opening a file never produces one.

pub use calc_macro_security::{FileOrigin, MacroBlock};

/// Permission to compile a workbook file's own VBA project and raise its events. Only the
/// constructors below make one; nothing derives it from opening a file.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ProjectGrant {
    reason: GrantReason,
    /// The policy bound to every runtime host prepared under this grant.
    policy: calc_macro_security::VbaPolicy,
}

/// Why a [`ProjectGrant`] was given.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum GrantReason {
    /// The person enabled macros for this file (the page's bar, or the desktop harness's
    /// `--enable-macros`).
    PersonEnabled,
    /// The person unblocked a marked or network file and enabled its macros, after the page's
    /// short warning (dc-15dk).
    PersonUnblocked(Option<MacroBlock>),
    /// The macro security level Unguarded, or `--trust-macros` or `DNACALC_TRUST_ALL_MACROS=1`,
    /// trusts every file; `by` names which (dc-15dk).
    TrustAll { by: String },
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
                policy: calc_macro_security::VbaPolicy::Full,
            }),
        }
    }

    /// The person unblocked `origin`'s file: the one route past a Mark of the Web or network
    /// block, taken only from the page's Unblock and enable button, behind its warning.
    pub fn person_unblocked(origin: FileOrigin) -> Self {
        Self {
            reason: GrantReason::PersonUnblocked(origin.block()),
            policy: calc_macro_security::VbaPolicy::Full,
        }
    }

    /// The macro security level Unguarded, or the flag or variable that pins it (`by` says which),
    /// trusts every file whatever its origin, with the VBA host policy `policy`.
    pub fn trust_all(by: &str, policy: calc_macro_security::VbaPolicy) -> Self {
        Self {
            reason: GrantReason::TrustAll { by: by.to_owned() },
            policy,
        }
    }

    /// The policy the VBA host enforces for the project this grant lets run.
    pub fn policy(&self) -> calc_macro_security::VbaPolicy {
        self.policy
    }

    /// An evidence run's trusted-code manifest lists the package. The caller has checked the
    /// package's bytes against `package_sha256` and found the manifest entry enabling its project.
    pub fn evidence_manifest(manifest_sha256: &str, package_sha256: &str) -> Self {
        Self {
            reason: GrantReason::EvidenceManifest {
                manifest_sha256: manifest_sha256.to_owned(),
                package_sha256: package_sha256.to_owned(),
            },
            policy: calc_macro_security::VbaPolicy::Full,
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
            (FileOrigin::Unknown, Some(MacroBlock::Unknown)),
        ] {
            assert_eq!(origin.block(), block, "{origin:?}");
            assert_eq!(ProjectGrant::person_enabled(origin).err(), block);
        }
        assert_eq!(
            ProjectGrant::person_enabled(FileOrigin::Local)
                .unwrap()
                .reason(),
            &GrantReason::PersonEnabled
        );
    }

    /// The unblock route and the trust-all route grant a blocked file; the person's plain enable
    /// still does not (dc-15dk).
    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn unblocking_and_trusting_all_grant_what_enabling_refuses() {
        assert_eq!(
            ProjectGrant::person_unblocked(FileOrigin::Zone(3)).reason(),
            &GrantReason::PersonUnblocked(Some(MacroBlock::Zone(3)))
        );
        assert_eq!(
            ProjectGrant::person_unblocked(FileOrigin::Network).reason(),
            &GrantReason::PersonUnblocked(Some(MacroBlock::Network))
        );
        assert_eq!(
            ProjectGrant::trust_all("--trust-macros", calc_macro_security::VbaPolicy::Full)
                .reason(),
            &GrantReason::TrustAll {
                by: "--trust-macros".into()
            }
        );
        assert!(ProjectGrant::person_enabled(FileOrigin::Network).is_err());
    }
}
