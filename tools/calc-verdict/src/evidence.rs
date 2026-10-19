//! The acquisition labels of a legacy evidence record (`dnacalc-evidence-record/1`, written by
//! `cargo xtask evidence labels` into `migration/evidence/oxfunc-smart-fuzzer.json`). Each entry
//! of the record names, in `certifies_exact_target`, the one Exact target it can serve as
//! acquisition evidence for (`excel-build-<build>`), or null when its Excel build is unknown and it
//! can certify none. A CaptureRecord converted from such an entry is certified only when its label
//! names the selected target ([`crate::judge`]).
//!
//! A label names an Excel build number, while a target's `excel_build` is the text its capture
//! recorded, which may be the build number alone or a longer build string. The two are compared
//! by build number, under this rule ([`excel_build_number`]):
//!
//! ```text
//! <B>                  the build number alone (20430)                      -> B
//! <M>.<m>.<B>.<R>      Excel's four-part file version (16.0.20430.20000)   -> B
//! anything else                                                            -> no build number
//! ```
//!
//! Every part is ASCII digits and the number is read in decimal, so leading zeros do not matter;
//! a label's `<build>` is read the same way. Two parts (`20430.20000`, the build and revision as
//! Excel's About box shows them) are not read: by its form alone that text cannot be told apart
//! from the version `16.0`, which names no build. A target with no build number is named by no
//! label.

use crate::DecodeError;
use crate::json::{self, Node};
use std::collections::BTreeMap;

pub const EVIDENCE_RECORD_FORMAT: &str = "dnacalc-evidence-record/1";

/// The prefix of a label that names an Excel build.
const BUILD_LABEL_PREFIX: &str = "excel-build-";

/// The Excel build number of a build text, by the rule of this module's documentation, or `None`
/// when the text has none.
pub(crate) fn excel_build_number(text: &str) -> Option<u64> {
    let parts: Vec<&str> = text.split('.').collect();
    if parts
        .iter()
        .any(|part| part.is_empty() || !part.bytes().all(|b| b.is_ascii_digit()))
    {
        return None;
    }
    let build = match parts[..] {
        [build] | [_, _, build, _] => build,
        _ => return None,
    };
    build.parse().ok()
}

/// A label that names a build: its text and the build number it names.
#[derive(Clone, Debug, PartialEq, Eq)]
struct BuildLabel {
    text: String,
    build: u64,
}

/// The label each entry of a legacy evidence record carries.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EvidenceLabels {
    labels: BTreeMap<String, Option<BuildLabel>>,
}

impl EvidenceLabels {
    /// The labels of the record in `bytes`. Only its `format` and each label's `entry` and
    /// `certifies_exact_target` are read; the record's other members are another tool's.
    pub fn decode(bytes: &[u8]) -> Result<EvidenceLabels, DecodeError> {
        let root = json::parse(bytes, "evidence")?;
        let members = json::members(&root, "evidence")?;
        let member = |name: &str| {
            members
                .iter()
                .find(|(member, _)| member == name)
                .map(|(_, node)| node)
                .ok_or_else(|| DecodeError::new("evidence", format!("has no member \"{name}\"")))
        };
        let format = json::text(member("format")?, "evidence.format")?;
        if format != EVIDENCE_RECORD_FORMAT {
            return Err(DecodeError::new(
                "evidence.format",
                format!("is {format:?}, expected {EVIDENCE_RECORD_FORMAT:?}"),
            ));
        }
        let mut labels = BTreeMap::new();
        for (i, node) in json::array(member("labels")?, "evidence.labels")?
            .iter()
            .enumerate()
        {
            let path = format!("evidence.labels[{i}]");
            let label = json::members(node, &path)?;
            let read = |name: &str| {
                label
                    .iter()
                    .find(|(member, _)| member == name)
                    .map(|(_, node)| node)
                    .ok_or_else(|| DecodeError::new(&path, format!("has no member \"{name}\"")))
            };
            let entry = json::name(read("entry")?, &format!("{path}.entry"))?;
            let target_at = format!("{path}.certifies_exact_target");
            let target = match read("certifies_exact_target")? {
                Node::Null => None,
                node => {
                    let target = json::text(node, &target_at)?;
                    let build = target
                        .strip_prefix(BUILD_LABEL_PREFIX)
                        .filter(|build| !build.is_empty() && build.bytes().all(|b| b.is_ascii_digit()))
                        .and_then(|build| build.parse::<u64>().ok())
                        .ok_or_else(|| {
                            DecodeError::new(
                                &target_at,
                                format!(
                                    "{target:?} is not {BUILD_LABEL_PREFIX}<digits> with a build number below 2^64"
                                ),
                            )
                        })?;
                    Some(BuildLabel {
                        text: target.to_owned(),
                        build,
                    })
                }
            };
            if labels.insert(entry.to_owned(), target).is_some() {
                return Err(DecodeError::new(
                    &path,
                    format!("labels the entry {entry:?} a second time"),
                ));
            }
        }
        Ok(EvidenceLabels { labels })
    }

    /// The entry's label: `None` when the record has no such entry, `Some(None)` when its build is
    /// unknown, `Some(Some("excel-build-<build>"))` otherwise.
    pub fn certifies_exact_target(&self, entry: &str) -> Option<Option<&str>> {
        self.labels
            .get(entry)
            .map(|label| label.as_ref().map(|label| label.text.as_str()))
    }

    /// Whether the entry's label names the build number of the target build text `excel_build`
    /// (compared by the rule of this module's documentation); `None` when the record has no such
    /// entry. A label of an unknown build, and a build text with no build number, name nothing.
    pub(crate) fn names_build(&self, entry: &str, excel_build: &str) -> Option<bool> {
        let target = excel_build_number(excel_build);
        self.labels.get(entry).map(|label| {
            label
                .as_ref()
                .is_some_and(|label| Some(label.build) == target)
        })
    }

    /// The number of labelled entries.
    pub fn len(&self) -> usize {
        self.labels.len()
    }

    pub fn is_empty(&self) -> bool {
        self.labels.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const RECORD: &str = r#"{"format": "dnacalc-evidence-record/1", "bead": "infra-m0.6",
        "labels": [
          {"entry": "cache/a/", "files": 3, "label": "known", "excel_build": "20430", "certifies_exact_target": "excel-build-20430"},
          {"entry": "cache/b.json", "files": 1, "label": "unknown", "excel_build": null, "certifies_exact_target": null}
        ]}"#;

    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn labels_name_one_build_or_none() {
        let labels = EvidenceLabels::decode(RECORD.as_bytes()).unwrap();
        assert_eq!(labels.len(), 2);
        assert_eq!(
            labels.certifies_exact_target("cache/a/"),
            Some(Some("excel-build-20430"))
        );
        assert_eq!(labels.certifies_exact_target("cache/b.json"), Some(None));
        assert_eq!(labels.certifies_exact_target("cache/c/"), None);
        assert_eq!(labels.names_build("cache/a/", "20430"), Some(true));
        assert_eq!(labels.names_build("cache/a/", "19929"), Some(false));
        assert_eq!(labels.names_build("cache/a/", "2043"), Some(false));
        assert_eq!(labels.names_build("cache/b.json", "20430"), Some(false));
        assert_eq!(labels.names_build("cache/c/", "20430"), None);
    }

    /// A full build string and its short form name the same build number.
    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn full_build_strings_reduce_to_their_build_number() {
        for (text, build) in [
            ("20430", Some(20430)),
            ("020430", Some(20430)),
            ("20430.20000", None),
            ("16.0.20430.20000", Some(20430)),
            ("16.0.19929.20106", Some(19929)),
            ("16.0", None),
            ("16.0.20430", None),
            ("16.0.20430.20000.1", None),
            ("16.0.20430.", None),
            ("Build 20430.20000", None),
            ("", None),
            ("99999999999999999999", None),
        ] {
            assert_eq!(excel_build_number(text), build, "{text:?}");
        }
        let labels = EvidenceLabels::decode(RECORD.as_bytes()).unwrap();
        assert_eq!(
            labels.names_build("cache/a/", "16.0.20430.20000"),
            Some(true)
        );
        assert_eq!(labels.names_build("cache/a/", "20430.20000"), Some(false));
        assert_eq!(labels.names_build("cache/a/", "20430"), Some(true));
        assert_eq!(
            labels.names_build("cache/a/", "16.0.19929.20106"),
            Some(false)
        );
        assert_eq!(labels.names_build("cache/a/", "16.0"), Some(false));
        assert_eq!(
            labels.names_build("cache/b.json", "16.0.20430.20000"),
            Some(false)
        );
        let leading_zero = RECORD.replace("excel-build-20430", "excel-build-020430");
        let labels = EvidenceLabels::decode(leading_zero.as_bytes()).unwrap();
        assert_eq!(
            labels.names_build("cache/a/", "16.0.20430.20000"),
            Some(true)
        );
    }

    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn malformed_records_are_refused() {
        for (record, refusal) in [
            (
                RECORD.replace("dnacalc-evidence-record/1", "dnacalc-evidence-record/2"),
                "expected",
            ),
            (
                RECORD.replace("\"cache/b.json\"", "\"cache/a/\""),
                "a second time",
            ),
            (
                RECORD.replace("\"excel-build-20430\"", "\"build-20430\""),
                "is not excel-build-<digits>",
            ),
            (
                RECORD.replace("\"excel-build-20430\"", "\"excel-build-\""),
                "is not excel-build-<digits>",
            ),
            (
                RECORD.replace(
                    "\"excel-build-20430\"",
                    "\"excel-build-99999999999999999999\"",
                ),
                "below 2^64",
            ),
            (
                RECORD.replace(", \"certifies_exact_target\": null", ""),
                "has no member \"certifies_exact_target\"",
            ),
        ] {
            let error = EvidenceLabels::decode(record.as_bytes()).err().unwrap();
            assert!(error.to_string().contains(refusal), "{error}");
        }
    }
}
