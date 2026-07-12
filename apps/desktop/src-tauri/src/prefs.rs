//! Persists the user-chosen vault root across launches. A tiny JSON file in the
//! Tauri app config dir holds the last-connected vault root; on startup the
//! desktop opens that vault (falling back to the platform default when no pref
//! exists yet). This is an app-level preference, kept separate from any vault's
//! own `config.toml`.

use std::fs;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Manager};

#[derive(Serialize, Deserialize)]
struct Prefs {
    #[serde(default)]
    vault_root: Option<String>,
}

fn prefs_path(app: &AppHandle) -> Option<PathBuf> {
    app.path()
        .app_config_dir()
        .ok()
        .map(|dir| dir.join("desktop-prefs.json"))
}

pub fn load_root(app: &AppHandle) -> Option<PathBuf> {
    let path = prefs_path(app)?;
    let text = fs::read_to_string(&path).ok()?;
    let prefs: Prefs = serde_json::from_str(&text).ok()?;
    prefs
        .vault_root
        .filter(|value| !value.is_empty())
        .map(PathBuf::from)
}

pub fn save_root(app: &AppHandle, root: &Path) -> std::io::Result<()> {
    let path = prefs_path(app).ok_or_else(|| {
        std::io::Error::new(
            std::io::ErrorKind::NotFound,
            "app config directory unavailable",
        )
    })?;
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let prefs = Prefs {
        vault_root: Some(root.display().to_string()),
    };
    let text = serde_json::to_string_pretty(&prefs)?;
    fs::write(&path, text)?;
    Ok(())
}
