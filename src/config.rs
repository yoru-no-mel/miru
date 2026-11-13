use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::filters::FilterMode;
use crate::viewer::FitMode;

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct PersistedState {
    #[serde(default)]
    pub window_width: Option<f32>,
    #[serde(default)]
    pub window_height: Option<f32>,
    /// Default fit mode applied to every image; persisted across sessions.
    #[serde(default)]
    pub fit_mode: Option<FitMode>,
    /// Default image scaling filter; persisted across sessions.
    #[serde(default)]
    pub filter_mode: Option<FilterMode>,
}

fn config_path() -> Option<PathBuf> {
    #[cfg(target_os = "windows")]
    {
        std::env::var_os("APPDATA").map(|b| PathBuf::from(b).join("miru").join("config.toml"))
    }
    #[cfg(not(target_os = "windows"))]
    {
        std::env::var_os("HOME").map(|h| {
            PathBuf::from(h)
                .join(".config")
                .join("miru")
                .join("config.toml")
        })
    }
}

pub fn load() -> PersistedState {
    let Some(path) = config_path() else {
        return PersistedState::default();
    };
    let Ok(text) = std::fs::read_to_string(&path) else {
        return PersistedState::default();
    };
    toml::from_str(&text).unwrap_or_default()
}

pub fn save(state: &PersistedState) {
    let Some(path) = config_path() else {
        return;
    };
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    if let Ok(text) = toml::to_string_pretty(state) {
        let _ = std::fs::write(path, text);
    }
}
