//! The numeric policy a calculation runs under (plan section 5). A [`NumericMode`] is a value the
//! caller supplies (a workbook's CalcHost metadata, a OneCalc session preference or a request's
//! explicit override); this package keeps no process-global mode.

use std::sync::Arc;

/// The Excel target an Exact calculation reproduces (plan section 5.2): the Excel build, its
/// release channel and compatibility setting, and the observed numeric environment (with the CPU
/// family where it matters). Each part is the text a capture manifest records for it; two targets
/// are the same target only when all four parts are equal.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct NumericTargetId {
    excel_build: Arc<str>,
    channel: Arc<str>,
    compatibility: Arc<str>,
    environment: Arc<str>,
}

impl NumericTargetId {
    /// The target with these four parts; `None` when any part is empty.
    pub fn new(
        excel_build: &str,
        channel: &str,
        compatibility: &str,
        environment: &str,
    ) -> Option<Self> {
        if [excel_build, channel, compatibility, environment]
            .iter()
            .any(|part| part.is_empty())
        {
            return None;
        }
        Some(NumericTargetId {
            excel_build: excel_build.into(),
            channel: channel.into(),
            compatibility: compatibility.into(),
            environment: environment.into(),
        })
    }

    pub fn excel_build(&self) -> &str {
        &self.excel_build
    }

    pub fn channel(&self) -> &str {
        &self.channel
    }

    pub fn compatibility(&self) -> &str {
        &self.compatibility
    }

    /// The observed numeric environment, with the CPU family where it matters.
    pub fn environment(&self) -> &str {
        &self.environment
    }
}

/// How numbers are calculated (plan section 5.1). `Fast`, the default everywhere, is the portable
/// deterministic policy; `Exact` reproduces one selected Excel target and is chosen deliberately.
#[derive(Clone, Debug, Default, PartialEq, Eq, Hash)]
pub enum NumericMode {
    #[default]
    Fast,
    Exact(NumericTargetId),
}

impl NumericMode {
    /// The Exact target, or `None` under Fast.
    pub fn target(&self) -> Option<&NumericTargetId> {
        match self {
            Self::Fast => None,
            Self::Exact(target) => Some(target),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn fast_is_the_default_and_exact_names_its_target() {
        assert_eq!(NumericMode::default(), NumericMode::Fast);
        assert_eq!(NumericMode::default().target(), None);
        let target =
            NumericTargetId::new("16.0.0.0", "channel", "compatibility", "x86-64").unwrap();
        assert_eq!(NumericMode::Exact(target.clone()).target(), Some(&target));
        assert_eq!(target.environment(), "x86-64");
        assert_eq!(
            NumericTargetId::new("", "channel", "compatibility", "x86-64"),
            None
        );
    }
}
