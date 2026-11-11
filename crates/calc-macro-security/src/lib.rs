//! The macro security level (dc-15dk): the one setting DNA Calc and DNA OneCalc share for whether a
//! workbook file's own VBA project runs.
//!
//! - [`MacroSecurity::Off`] (the default, Excel's): a macro workbook opens with its project held,
//!   a bar offers Enable macros for this file, and a file with the Mark of the Web or from the
//!   network offers Unblock and enable behind a short warning.
//! - [`MacroSecurity::Safe`]: VBA runs with dangerous operating-system services refused. The
//!   policy is built in CalcVBA (dc-10un); until it lands ([`SAFE_AVAILABLE`]) Safe behaves as Off
//!   and says so ([`MacroPolicy::safe_notice`]).
//! - [`MacroSecurity::Unguarded`]: the project runs on open with full access, Excel's enable
//!   macros, no prompt; a file that would have been blocked runs too, with an information notice.
//!
//! A script, an agent or the test harness pins the level for one run with `--macros
//! off|safe|unguarded`, or Unguarded with `--trust-macros` or `DNACALC_TRUST_ALL_MACROS=1`
//! ([`MacroPolicy::pinned`]); a pinned level is shown, not stored, and the page cannot change it.
//! The level is stored per user by the native hosts (`calc_platform_desktop::macro_security`).

use serde::{Deserialize, Serialize};

/// The command-line flag that pins the level to Unguarded.
pub const TRUST_FLAG: &str = "--trust-macros";
/// The command-line option that pins the level to the one it names.
pub const MACROS_FLAG: &str = "--macros";
/// The environment variable that pins the level to Unguarded when it is `1`.
pub const TRUST_ENV: &str = "DNACALC_TRUST_ALL_MACROS";

/// Whether the Safe policy exists in the VBA host yet (dc-10un). While it does not, Safe behaves
/// as Off.
pub const SAFE_AVAILABLE: bool = false;

/// What the VBA host is asked to enforce for a project that runs: the single policy choice passed
/// to it (a placeholder for dc-10un's `PolicyHost` Safe/Full).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum VbaPolicy {
    /// Dangerous operating-system services refused.
    Safe,
    /// Full access.
    Full,
}

/// Whether a workbook file's own VBA project may run.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum MacroSecurity {
    /// Held on open; the person enables it (a marked or network file: unblocks it) per file.
    #[default]
    Off,
    /// Runs with dangerous operating-system services refused (not available yet: acts as Off).
    Safe,
    /// Runs on open with full access, whatever the file's origin.
    Unguarded,
}

impl MacroSecurity {
    pub const ALL: [Self; 3] = [Self::Off, Self::Safe, Self::Unguarded];

    /// The stored and spoken form: `off`, `safe`, `unguarded`.
    pub fn key(self) -> &'static str {
        match self {
            Self::Off => "off",
            Self::Safe => "safe",
            Self::Unguarded => "unguarded",
        }
    }

    /// The name the page shows.
    pub fn label(self) -> &'static str {
        match self {
            Self::Off => "Off",
            Self::Safe => "Safe",
            Self::Unguarded => "Unguarded",
        }
    }

    /// The level a stored or typed key names (case and surrounding space ignored).
    pub fn parse(text: &str) -> Option<Self> {
        let text = text.trim();
        Self::ALL
            .into_iter()
            .find(|level| level.key().eq_ignore_ascii_case(text))
    }

    /// The policy the VBA host is asked to enforce when a project runs at this level; `None` when
    /// nothing runs on open (Off, and Safe until it is available).
    pub fn vba_policy(self) -> Option<VbaPolicy> {
        match self {
            Self::Off => None,
            Self::Safe => SAFE_AVAILABLE.then_some(VbaPolicy::Safe),
            Self::Unguarded => Some(VbaPolicy::Full),
        }
    }
}

/// What pinned the level for this run.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Pin {
    /// `--trust-macros` (Unguarded).
    Flag,
    /// `DNACALC_TRUST_ALL_MACROS=1` (Unguarded).
    Environment,
    /// `--macros LEVEL`.
    Option,
}

impl Pin {
    pub fn label(self) -> &'static str {
        match self {
            Self::Flag => TRUST_FLAG,
            Self::Environment => TRUST_ENV,
            Self::Option => MACROS_FLAG,
        }
    }
}

/// Whether the variable's value pins the level: exactly `1`.
pub fn env_pins(value: Option<&str>) -> bool {
    value.is_some_and(|value| value.trim() == "1")
}

/// The level in force: the person's stored level, or a level pinned by the command line or the
/// environment (`--macros` first, then `--trust-macros`, then the variable).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct MacroPolicy {
    level: MacroSecurity,
    pinned: Option<Pin>,
}

impl MacroPolicy {
    /// The person's stored level, nothing pinned.
    pub fn stored(level: MacroSecurity) -> Self {
        Self {
            level,
            pinned: None,
        }
    }

    /// The stored level, unless `--macros` (`option`), `--trust-macros` (`flag`) or the variable
    /// pins another over it.
    pub fn resolve(
        stored: MacroSecurity,
        flag: bool,
        env: Option<&str>,
        option: Option<MacroSecurity>,
    ) -> Self {
        let pinned = |level, pin| Self {
            level,
            pinned: Some(pin),
        };
        if let Some(level) = option {
            pinned(level, Pin::Option)
        } else if flag {
            pinned(MacroSecurity::Unguarded, Pin::Flag)
        } else if env_pins(env) {
            pinned(MacroSecurity::Unguarded, Pin::Environment)
        } else {
            Self::stored(stored)
        }
    }

    /// The level in force.
    pub fn level(&self) -> MacroSecurity {
        self.level
    }

    /// What pinned the level, if anything; the page cannot change a pinned level.
    pub fn pinned(&self) -> Option<Pin> {
        self.pinned
    }

    /// The person's choice in the page: refused while pinned.
    pub fn choose(&mut self, level: MacroSecurity) -> Result<(), String> {
        match self.pinned {
            Some(pin) => Err(format!(
                "The macro security level is pinned to {} by {}",
                self.level.label(),
                pin.label()
            )),
            None => {
                self.level = level;
                Ok(())
            }
        }
    }

    /// Whether a workbook's project runs as it opens, and with which policy.
    pub fn runs_on_open(&self) -> Option<VbaPolicy> {
        self.level.vba_policy()
    }

    /// Why Safe does not run anything yet, when it is the level; `None` otherwise.
    pub fn safe_notice(&self) -> Option<&'static str> {
        (self.level == MacroSecurity::Safe && !SAFE_AVAILABLE).then_some(
            "Safe mode is not available yet, so macros stay off (you can still enable them for this file)",
        )
    }

    /// Who trusts a project that runs without the person's click, for the grant's record.
    pub fn trust_source(&self) -> &'static str {
        match self.pinned {
            Some(pin) => pin.label(),
            None => "the macro security level",
        }
    }

    /// The line the page always shows: the level, and what pins it.
    pub fn summary(&self) -> String {
        match self.pinned {
            Some(pin) => format!("Macro security: {} ({})", self.level.label(), pin.label()),
            None => format!("Macro security: {}", self.level.label()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn keys_round_trip_and_off_is_the_default() {
        assert_eq!(MacroSecurity::default(), MacroSecurity::Off);
        for level in MacroSecurity::ALL {
            assert_eq!(MacroSecurity::parse(level.key()), Some(level));
        }
        assert_eq!(
            MacroSecurity::parse(" UnGuarded\n"),
            Some(MacroSecurity::Unguarded)
        );
        assert_eq!(MacroSecurity::parse("yes"), None);
    }

    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn each_level_decides_what_runs_on_open() {
        let off = MacroPolicy::stored(MacroSecurity::Off);
        assert_eq!((off.runs_on_open(), off.safe_notice()), (None, None));
        let all = MacroPolicy::stored(MacroSecurity::Unguarded);
        assert_eq!(all.runs_on_open(), Some(VbaPolicy::Full));
        // Safe is the placeholder until dc-10un: nothing runs and the page says why.
        let safe = MacroPolicy::stored(MacroSecurity::Safe);
        assert_eq!(
            safe.runs_on_open(),
            SAFE_AVAILABLE.then_some(VbaPolicy::Safe)
        );
        assert_eq!(safe.safe_notice().is_some(), !SAFE_AVAILABLE);
    }

    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn the_flags_and_the_variable_pin_a_level_over_the_stored_one() {
        let stored = MacroSecurity::Safe;
        let plain = MacroPolicy::resolve(stored, false, None, None);
        assert_eq!((plain.level(), plain.pinned()), (stored, None));
        assert_eq!(
            MacroPolicy::resolve(stored, false, Some("0"), None).pinned(),
            None
        );
        let flag = MacroPolicy::resolve(stored, true, Some("1"), None);
        assert_eq!(
            (flag.level(), flag.pinned()),
            (MacroSecurity::Unguarded, Some(Pin::Flag))
        );
        let env = MacroPolicy::resolve(stored, false, Some("1"), None);
        assert_eq!(
            (env.level(), env.pinned()),
            (MacroSecurity::Unguarded, Some(Pin::Environment))
        );
        assert!(env.summary().contains(TRUST_ENV));
        let option = MacroPolicy::resolve(stored, true, Some("1"), Some(MacroSecurity::Off));
        assert_eq!(
            (option.level(), option.pinned()),
            (MacroSecurity::Off, Some(Pin::Option))
        );
        let mut pinned = env;
        assert!(pinned.choose(MacroSecurity::Off).is_err());
        assert_eq!(pinned.level(), MacroSecurity::Unguarded);
        let mut free = plain;
        free.choose(MacroSecurity::Off).unwrap();
        assert_eq!(free.summary(), "Macro security: Off");
    }
}
