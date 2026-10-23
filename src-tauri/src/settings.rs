//! User settings, stored as `settings.json` in the app data dir.

use std::fs;
use std::path::Path;

use serde::{Deserialize, Serialize};

const FILE: &str = "settings.json";

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Settings {
    /// Notify when the last real (non-dry) run is at least this old; 0 = always.
    pub remind_after_days: u32,
    /// Start hidden in the tray at login.
    pub autostart: bool,
    /// Default flags, also used by the tray's "Run now".
    pub cleanup: bool,
    pub assume_yes: bool,
    /// Terminal font size in px.
    pub font_size: u32,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            remind_after_days: 7,
            autostart: true,
            cleanup: false,
            assume_yes: false,
            font_size: 13,
        }
    }
}

/// Loaded settings, and whether the file existed (false on first launch).
pub fn load(dir: &Path) -> (Settings, bool) {
    match fs::read(dir.join(FILE)) {
        Ok(json) => (serde_json::from_slice(&json).unwrap_or_default(), true),
        Err(_) => (Settings::default(), false),
    }
}

pub fn save(dir: &Path, settings: &Settings) -> std::io::Result<()> {
    fs::create_dir_all(dir)?;
    fs::write(dir.join(FILE), serde_json::to_vec_pretty(settings)?)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn missing_fields_use_defaults() {
        let settings: Settings = serde_json::from_str(r#"{"cleanup": true}"#).unwrap();
        assert_eq!(
            settings,
            Settings {
                cleanup: true,
                ..Settings::default()
            }
        );
    }

    #[test]
    fn round_trip() {
        let dir =
            std::env::temp_dir().join(format!("topgrade-gui-settings-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        assert_eq!(load(&dir), (Settings::default(), false));
        let settings = Settings {
            remind_after_days: 3,
            ..Settings::default()
        };
        save(&dir, &settings).unwrap();
        assert_eq!(load(&dir), (settings, true));
        fs::remove_dir_all(&dir).unwrap();
    }
}
