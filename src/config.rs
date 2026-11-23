use std::path::PathBuf;

use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct AppConfig {
    pub always_on_top: bool,
    pub dark_mode: bool,
    pub show_preview: bool,
    pub nudge_mode: bool,
    pub nudge_ms: u64,
    pub last_device_path: Option<String>,
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            always_on_top: false,
            dark_mode: true,
            show_preview: true,
            nudge_mode: false,
            nudge_ms: 150,
            last_device_path: None,
        }
    }
}

impl AppConfig {
    pub fn load() -> Self {
        let Some(path) = config_path() else {
            return Self::default();
        };
        let Ok(text) = std::fs::read_to_string(&path) else {
            return Self::default();
        };
        serde_json::from_str(&text).unwrap_or_else(|err| {
            log::warn!("failed to parse config at {}: {err}", path.display());
            Self::default()
        })
    }

    pub fn save(&self) {
        let Some(path) = config_path() else {
            return;
        };
        if let Some(parent) = path.parent()
            && let Err(err) = std::fs::create_dir_all(parent)
        {
            log::warn!("failed to create config dir {}: {err}", parent.display());
            return;
        }
        match serde_json::to_string_pretty(self) {
            Ok(json) => {
                if let Err(err) = std::fs::write(&path, json) {
                    log::warn!("failed to write config at {}: {err}", path.display());
                }
            }
            Err(err) => log::warn!("failed to serialize config: {err}"),
        }
    }
}

fn config_path() -> Option<PathBuf> {
    if let Ok(local_app_data) = std::env::var("LOCALAPPDATA")
        && !local_app_data.is_empty()
    {
        return Some(
            PathBuf::from(local_app_data)
                .join("ptz-controller")
                .join("config.json"),
        );
    }
    std::env::current_exe()
        .ok()
        .and_then(|exe| exe.parent().map(|p| p.join("ptz-controller-config.json")))
}
