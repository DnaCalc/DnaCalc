//! What the desktop app remembers between runs: where its window was and the files the person
//! opened or saved most recently. One small JSON file, `settings.json`, in the app's data folder
//! (`%LOCALAPPDATA%\DNA OneCalc`, or the folder `--data-dir` names).
//!
//! Nothing here is needed to run: a missing or damaged file is the same as a first run, and a
//! file that cannot be written is only noted on stderr.

use calc_platform_desktop::Placement;
use std::path::{Path, PathBuf};

/// The most files the recent list keeps.
pub const MOST_RECENT: usize = 8;

/// The remembered window place and recent files, and the folder they live in.
#[derive(Debug)]
pub struct Settings {
    folder: PathBuf,
    pub window: Option<Placement>,
    pub recent: Vec<String>,
}

/// The data folder when none is named: `%LOCALAPPDATA%\DNA OneCalc`, else a folder under the
/// temporary one.
pub fn default_folder() -> PathBuf {
    std::env::var_os("LOCALAPPDATA")
        .map(|base| PathBuf::from(base).join("DNA OneCalc"))
        .unwrap_or_else(|| std::env::temp_dir().join("DNA OneCalc"))
}

impl Settings {
    /// The settings in `folder`; none yet (or a file that does not read) gives the first-run ones.
    pub fn load(folder: &Path) -> Self {
        let mut settings = Settings {
            folder: folder.to_owned(),
            window: None,
            recent: Vec::new(),
        };
        let Ok(text) = std::fs::read_to_string(folder.join("settings.json")) else {
            return settings;
        };
        let Ok(json) = serde_json::from_str::<serde_json::Value>(&text) else {
            return settings;
        };
        if let Some(w) = json.get("window") {
            let number = |key: &str| w.get(key).and_then(serde_json::Value::as_i64).map(|n| n as i32);
            if let (Some(left), Some(top), Some(right), Some(bottom)) =
                (number("left"), number("top"), number("right"), number("bottom"))
            {
                settings.window = Some(Placement {
                    left,
                    top,
                    right,
                    bottom,
                    maximized: w.get("maximized").and_then(serde_json::Value::as_bool).unwrap_or(false),
                });
            }
        }
        if let Some(list) = json.get("recent").and_then(serde_json::Value::as_array) {
            settings.recent = list
                .iter()
                .filter_map(|path| path.as_str().map(str::to_owned))
                .take(MOST_RECENT)
                .collect();
        }
        settings
    }

    /// The folder the settings and the app's other data live in.
    pub fn folder(&self) -> &Path {
        &self.folder
    }

    /// Writes the settings; a failure is noted, never raised.
    pub fn save(&self) {
        let window = self.window.map(|p| {
            serde_json::json!({
                "left": p.left, "top": p.top, "right": p.right, "bottom": p.bottom,
                "maximized": p.maximized,
            })
        });
        let text = serde_json::json!({ "window": window, "recent": self.recent });
        let written = std::fs::create_dir_all(&self.folder).and_then(|()| {
            std::fs::write(
                self.folder.join("settings.json"),
                serde_json::to_string_pretty(&text).unwrap_or_default(),
            )
        });
        if let Err(why) = written {
            eprintln!("dna-onecalc-desktop: settings not saved: {why}");
        }
    }

    /// Notes that `path` was opened or saved: it is the newest recent file, once.
    pub fn push_recent(&mut self, path: &str) {
        self.recent.retain(|held| !held.eq_ignore_ascii_case(path));
        self.recent.insert(0, path.to_owned());
        self.recent.truncate(MOST_RECENT);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn folder(name: &str) -> PathBuf {
        let folder = std::env::temp_dir().join(format!("onecalc-settings-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&folder);
        folder
    }

    /// Native-only: the window place and the recent files survive a save and a load; a folder
    /// with nothing in it, or a damaged file, is a first run.
    #[test]
    fn settings_round_trip_and_a_damaged_file_is_a_first_run() {
        let folder = folder("round");
        assert!(Settings::load(&folder).window.is_none());
        let mut settings = Settings::load(&folder);
        settings.window = Some(Placement { left: 10, top: -20, right: 810, bottom: 620, maximized: true });
        settings.push_recent("C:\\a\\one.xlsx");
        settings.push_recent("C:\\a\\two.xlsm");
        settings.save();
        let again = Settings::load(&folder);
        assert_eq!(again.window, settings.window);
        assert_eq!(again.recent, ["C:\\a\\two.xlsm", "C:\\a\\one.xlsx"]);
        std::fs::write(folder.join("settings.json"), "{ not json").unwrap();
        let damaged = Settings::load(&folder);
        assert!(damaged.window.is_none() && damaged.recent.is_empty());
        std::fs::remove_dir_all(&folder).unwrap();
    }

    /// Native-only: a file opened again moves to the top once (any letter case of the same path),
    /// and the list keeps the most recent eight.
    #[test]
    fn recent_files_are_newest_first_without_repeats_and_limited() {
        let mut settings = Settings::load(&folder("recent"));
        for n in 0..12 {
            settings.push_recent(&format!("C:\\f\\{n}.xlsx"));
        }
        assert_eq!(settings.recent.len(), MOST_RECENT);
        assert_eq!(settings.recent[0], "C:\\f\\11.xlsx");
        settings.push_recent("c:\\F\\8.XLSX");
        assert_eq!(settings.recent[0], "c:\\F\\8.XLSX");
        assert_eq!(settings.recent.iter().filter(|p| p.to_ascii_lowercase() == "c:\\f\\8.xlsx").count(), 1);
        assert_eq!(settings.recent.len(), MOST_RECENT);
    }
}
