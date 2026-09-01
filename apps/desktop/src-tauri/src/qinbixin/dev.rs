use std::{collections::HashMap, fs, path::PathBuf};

use tauri::AppHandle;

use super::{
    commands::login_with_credentials,
    environment::{load_environment, read_session, save_environment, state_path},
    types::{QinbixinDevAccount, QinbixinEnvironment, QinbixinSession, QinbixinStatusDto},
};
use crate::state::AppState;

fn dotenv_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../../.env")
}

fn read_env_map() -> Result<HashMap<String, String>, String> {
    let text = fs::read_to_string(dotenv_path())
        .map_err(|e| crate::logging::log_warn(format!("unable to read dev credentials: {e}")))?;
    Ok(text
        .lines()
        .filter_map(|line| {
            let (key, value) = line.split_once('=')?;
            Some((
                key.trim().to_owned(),
                value.trim().trim_matches('"').to_owned(),
            ))
        })
        .collect())
}

fn parse_dev_accounts(env: &HashMap<String, String>) -> Vec<QinbixinDevAccount> {
    (1..=3)
        .filter_map(|index| {
            let id = index.to_string();
            let user_name = env
                .get(&format!("DEV_QBX_ID_{index}"))
                .map(|value| value.trim())
                .unwrap_or_default();
            let password = env
                .get(&format!("DEV_QBX_PASSWORD_{index}"))
                .map(|value| value.trim())
                .unwrap_or_default();
            (!user_name.is_empty() && !password.is_empty()).then(|| QinbixinDevAccount {
                id,
                user_name: user_name.to_owned(),
            })
        })
        .collect()
}

fn status_snapshot(
    environment: QinbixinEnvironment,
    session: &QinbixinSession,
) -> QinbixinStatusDto {
    QinbixinStatusDto {
        logged_in: !session.token.is_empty(),
        profile: if session.token.is_empty() {
            None
        } else {
            Some(session.profile.clone())
        },
        has_unread: false,
        environment,
    }
}

#[tauri::command]
pub async fn qinbixin_set_environment(
    app: AppHandle,
    state: tauri::State<'_, AppState>,
    environment: QinbixinEnvironment,
) -> Result<QinbixinStatusDto, String> {
    let current = load_environment(&app);
    if current != environment {
        save_environment(&app, environment)?;
        let path = state_path(&app)?;
        *state.qinbixin.lock().unwrap() = read_session(&path).unwrap_or_default();
    }
    Ok(status_snapshot(
        environment,
        &state.qinbixin.lock().unwrap(),
    ))
}

#[tauri::command]
pub async fn qinbixin_dev_accounts() -> Result<Vec<QinbixinDevAccount>, String> {
    Ok(parse_dev_accounts(&read_env_map()?))
}

#[tauri::command]
pub async fn qinbixin_login_dev_account(
    app: AppHandle,
    state: tauri::State<'_, AppState>,
    account_id: String,
) -> Result<QinbixinStatusDto, String> {
    let env = read_env_map()?;
    let index = account_id
        .parse::<usize>()
        .map_err(|_| crate::logging::log_warn("invalid dev account"))?;
    let user_name = env
        .get(&format!("DEV_QBX_ID_{index}"))
        .map(|value| value.trim())
        .unwrap_or_default();
    let password = env
        .get(&format!("DEV_QBX_PASSWORD_{index}"))
        .map(|value| value.trim())
        .unwrap_or_default();
    if user_name.is_empty() || password.is_empty() {
        return Err(crate::logging::log_warn("missing dev account credentials"));
    }
    login_with_credentials(&app, &state, user_name.to_owned(), password.to_owned()).await
}
