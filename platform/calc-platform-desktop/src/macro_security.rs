//! The per-user macro security level, stored (dc-15dk). One small text file,
//! `macro-security.txt`, holds the level's key; both desktop applications read and write the same
//! one in the per-user settings folder (`%LOCALAPPDATA%\DnaCalc`, or the folder
//! `DNACALC_SETTINGS_DIR` names), so the level is one choice for the person, as Excel's trust
//! centre is. A missing or damaged file is the default, Off.
//!
//! `--trust-macros` and `DNACALC_TRUST_ALL_MACROS=1` pin a level over the stored level
//! ([`calc_macro_security::MacroPolicy::resolve`]); a pinned level is never written here.

use calc_macro_security::{MacroPolicy, MacroSecurity, TRUST_ENV};
use std::path::{Path, PathBuf};

/// The environment variable that names the settings folder (scripts and tests).
pub const FOLDER_ENV: &str = "DNACALC_SETTINGS_DIR";

const FILE: &str = "macro-security.txt";

/// The per-user settings folder: `DNACALC_SETTINGS_DIR`, else `%LOCALAPPDATA%\DnaCalc`, else a
/// folder under the temporary one.
pub fn default_folder() -> PathBuf {
    let named = |name: &str| std::env::var_os(name).filter(|v| !v.is_empty());
    named(FOLDER_ENV)
        .map(PathBuf::from)
        .or_else(|| named("LOCALAPPDATA").map(|base| PathBuf::from(base).join("DnaCalc")))
        .unwrap_or_else(|| std::env::temp_dir().join("DnaCalc"))
}

/// The stored level in `folder`; none yet, or a file that does not name a level, is Off.
pub fn load(folder: &Path) -> MacroSecurity {
    std::fs::read_to_string(folder.join(FILE))
        .ok()
        .and_then(|text| MacroSecurity::parse(&text))
        .unwrap_or_default()
}

/// Stores the level in `folder`.
pub fn save(folder: &Path, level: MacroSecurity) -> std::io::Result<()> {
    std::fs::create_dir_all(folder)?;
    std::fs::write(folder.join(FILE), format!("{}\n", level.key()))
}

/// The level in force at start-up: the stored one, or the level pinned by `--trust-macros`
/// (`flag`) or `DNACALC_TRUST_ALL_MACROS=1` (Unguarded), or by `--macros LEVEL` (`option`).
pub fn policy(folder: &Path, flag: bool, option: Option<MacroSecurity>) -> MacroPolicy {
    MacroPolicy::resolve(
        load(folder),
        flag,
        std::env::var(TRUST_ENV).ok().as_deref(),
        option,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn folder(name: &str) -> PathBuf {
        let folder =
            std::env::temp_dir().join(format!("macro-security-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&folder);
        folder
    }

    /// Native-only: it writes the per-user file. Each level survives a restart (a second read),
    /// and a first run, an empty file and a damaged one are Off.
    #[test]
    fn the_level_persists_per_user_and_defaults_to_ask() {
        let folder = folder("persist");
        assert_eq!(load(&folder), MacroSecurity::Off);
        for level in MacroSecurity::ALL {
            save(&folder, level).unwrap();
            assert_eq!(load(&folder), level);
        }
        std::fs::write(folder.join(FILE), "").unwrap();
        assert_eq!(load(&folder), MacroSecurity::Off);
        std::fs::write(folder.join(FILE), "\u{0}garbage").unwrap();
        assert_eq!(load(&folder), MacroSecurity::Off);
        let _ = std::fs::remove_dir_all(&folder);
    }

    /// Native-only: it writes the per-user file. The flag pins Unguarded over a stored level and
    /// leaves the stored level as it was.
    #[test]
    fn the_flag_pins_enable_all_without_touching_the_stored_level() {
        let folder = folder("flag");
        save(&folder, MacroSecurity::Safe).unwrap();
        let pinned = policy(&folder, true, None);
        assert_eq!(pinned.level(), MacroSecurity::Unguarded);
        assert!(pinned.pinned().is_some());
        assert_eq!(load(&folder), MacroSecurity::Safe);
        let _ = std::fs::remove_dir_all(&folder);
    }
}
