//! The macro security level (dc-15dk): the one setting DNA Calc and DNA OneCalc share for whether a
//! workbook file's own VBA project runs.
//!
//! - [`MacroSecurity::Off`] (the default, Excel's): a macro workbook opens with its project held,
//!   a bar offers Enable macros for this file, and a file with the Mark of the Web or from the
//!   network offers Unblock and enable behind a short warning.
//! - [`MacroSecurity::Safe`]: the project runs on open with dangerous operating-system services
//!   refused (CalcVBA's `PolicyMode::Safe`, dc-10un).
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

/// What the VBA host is asked to enforce for a project that runs: the single policy choice passed
/// to it, CalcVBA's `PolicyMode` (dc-10un).
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
    /// Runs on open with dangerous operating-system services refused.
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
    /// nothing runs on open (Off).
    pub fn vba_policy(self) -> Option<VbaPolicy> {
        match self {
            Self::Off => None,
            Self::Safe => Some(VbaPolicy::Safe),
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
        assert_eq!(off.runs_on_open(), None);
        let all = MacroPolicy::stored(MacroSecurity::Unguarded);
        assert_eq!(all.runs_on_open(), Some(VbaPolicy::Full));
        // Safe runs the project on open with the Safe policy; Unguarded with Full.
        let safe = MacroPolicy::stored(MacroSecurity::Safe);
        assert_eq!(safe.runs_on_open(), Some(VbaPolicy::Safe));
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

/// Where an opened workbook file came from, as far as its host can tell.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum FileOrigin {
    /// The host cannot verify the origin or read a valid Mark of the Web; requires Unblock.
    #[default]
    Unknown,
    /// A local file without a Mark of the Web.
    Local,
    /// A file whose Mark of the Web names this URL security zone (0 to 4).
    Zone(u32),
    /// A file opened from a network path (`\\server\share\...`).
    Network,
}

/// Why a file's project cannot be enabled.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum MacroBlock {
    /// The origin or Mark of the Web could not be verified.
    Unknown,
    /// The Mark of the Web names the internet (3) or restricted (4) zone.
    Zone(u32),
    /// The file was opened from the network.
    Network,
}

impl FileOrigin {
    /// What stops the person enabling the file's project, if anything: zones 3 and 4, and the
    /// network or an unverified origin. Zones 0 to 2 (this computer, the intranet, trusted sites) and unmarked files open
    /// disabled and can be enabled.
    pub fn block(self) -> Option<MacroBlock> {
        match self {
            Self::Zone(zone @ 3..) => Some(MacroBlock::Zone(zone)),
            Self::Network => Some(MacroBlock::Network),
            Self::Unknown => Some(MacroBlock::Unknown),
            Self::Zone(_) | Self::Local => None,
        }
    }
}

impl MacroBlock {
    pub fn origin_description(self) -> String {
        match self {
            Self::Unknown => "The file origin could not be verified".to_owned(),
            Self::Network => "This file was opened from the network".to_owned(),
            Self::Zone(zone) => format!(
                "This file carries the Mark of the Web (zone {zone}: it came from the internet or another computer)"
            ),
        }
    }
}

impl std::fmt::Display for MacroBlock {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Unknown => f.write_str("The file origin could not be verified; unblock it before enabling its VBA project"),
            Self::Zone(zone) => write!(
                f,
                "This file carries the Mark of the Web (zone {zone}: it came from the internet or another computer), so its VBA project is held and not run until you unblock it"
            ),
            Self::Network => f.write_str(
                "This file was opened from the network, so its VBA project is held and not run until you unblock it",
            ),
        }
    }
}
