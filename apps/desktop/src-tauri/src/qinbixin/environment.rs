use std::{
    fs,
    path::{Path, PathBuf},
    sync::Mutex,
};

use tauri::{AppHandle, Manager};

use super::types::{QinbixinEnvironment, QinbixinEnvironmentFile, QinbixinSession};

const ENVIRONMENT_FILE: &str = "qinbixin-environment.json";

pub(super) fn environment_path(app: &AppHandle) -> Result<PathBuf, String> {
    app.path()
        .app_config_dir()
        .map(|dir| dir.join(ENVIRONMENT_FILE))
        .map_err(|e| format!("unable to resolve app config dir: {e}"))
}

pub(super) fn load_environment(app: &AppHandle) -> QinbixinEnvironment {
    let Ok(path) = environment_path(app) else {
        return QinbixinEnvironment::default();
    };
    let Ok(text) = fs::read_to_string(path) else {
        return QinbixinEnvironment::default();
    };
    if let Ok(value) = serde_json::from_str::<QinbixinEnvironmentFile>(&text) {
        return value.environment;
    }
    QinbixinEnvironment::default()
}

#[cfg(debug_assertions)]
pub(super) fn save_environment(
    app: &AppHandle,
    environment: QinbixinEnvironment,
) -> Result<(), String> {
    let path = environment_path(app)?;
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|e| format!("unable to create config dir: {e}"))?;
    }
    let text = serde_json::to_string(&QinbixinEnvironmentFile { environment })
        .map_err(|e| format!("unable to encode environment: {e}"))?;
    fs::write(path, text).map_err(|e| format!("unable to save environment: {e}"))
}

pub(super) fn state_path(app: &AppHandle) -> Result<PathBuf, String> {
    app.path()
        .app_config_dir()
        .map(|dir| dir.join(load_environment(app).state_file()))
        .map_err(|e| format!("unable to resolve app config dir: {e}"))
}

pub(super) fn read_session(path: &Path) -> Option<QinbixinSession> {
    let text = fs::read_to_string(path).ok()?;
    serde_json::from_str(&text).ok()
}

pub fn load_session(app: &AppHandle, session: &Mutex<QinbixinSession>) {
    let Ok(path) = state_path(app) else {
        return;
    };
    if let Some(value) = read_session(&path) {
        *session.lock().unwrap() = value;
    }
}

pub(super) fn save_session(app: &AppHandle, session: &QinbixinSession) -> Result<(), String> {
    let path = state_path(app)?;
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|e| format!("unable to create config dir: {e}"))?;
    }
    let text =
        serde_json::to_string(session).map_err(|e| format!("unable to encode session: {e}"))?;
    fs::write(path, text).map_err(|e| format!("unable to save session: {e}"))
}

pub(super) fn store_new_token(
    app: &AppHandle,
    session: &Mutex<QinbixinSession>,
    token: &Option<String>,
) -> Result<(), String> {
    if let Some(new_token) = token {
        let mut guard = session.lock().unwrap();
        if *new_token != guard.token {
            guard.token = new_token.clone();
            let snapshot = guard.clone();
            drop(guard);
            save_session(app, &snapshot)?;
        }
    }
    Ok(())
}
